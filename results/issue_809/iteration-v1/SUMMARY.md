# Result — issue 809 / restore-session-fence

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: restore fences no session, and cannot report what it skipped. A restored image
  can resurrect an `Open` or `Completing` session whose bytes are gone, and nothing stops it
  from completing over them (D-B, `0016:717-728`, F13). A `Completing` session that had already
  written segments needs its `seg:` records retired in the **same** batch as its fence, or they
  have no deleter anywhere in the design (X57, `0016:880`) — and today the session record
  cannot even name them (`multipart.rs:2008-2038`). Separately, restore skips staged fragments
  silently through #803's gate (`crates/custodian/src/restore.rs:438`); 0016 requires the
  report to say so — `staged_skipped` and `sessions_fenced` beside `pending_skipped`
  (`0016:823`; `RestoreReport`, `restore.rs:115-215`). #803 also left one question here, marked
  `// deferred: #664` at `restore.rs:819`: whether a held (untrusted) staged record sets
  `needs_human()` — answered at #664's plan revision: no, but the report names it (leg H-iii).
- Success criterion: the NEW file `crates/custodian/tests/restore_session_fence.rs` passes
  over in-memory doubles, with records seeded as raw JSON. A `Completing` fixture carries the
  new nonce field; base decoding rejects it (`#[serde(deny_unknown_fields)]`), which is harmless
  there because base restore never reads `mpu:`. Legs:
  **(E) Restore reports staged skips.** `reconcile_after_restore` over a store with two staged
  fragments reports them as staged-skipped, separately from `pending_skipped`. Assert through
  the report's `Debug` rendering, which must contain `staged_skipped: 2` (`0016:823`); the base
  rendering has no such counter.
  **(F) Restore fences a resurrected `Open` session (D-B).** An `Open@E` session ends as
  `Aborting@E+1`. In the same batch — assert atomicity with a double that fails that one
  commit, after which **none** of the writes are present — its byte-retirement obligation is
  installed. Round-trip every obligation the fence writes through `decode_retire_obligation`
  (`multipart.rs:3455`) against the key it sits under. The `Debug` rendering contains
  `sessions_fenced: 1`. A Complete retried against that session cannot fence it, since the
  Complete fence requires `Open@E` (`0016:660`); the client-visible `4xx` is #658's.
  **(G) Restore fences a resurrected `Completing` session with its segments' deleter (X57).** A
  `Completing@E` session with `segments_written > 0`, its nonce on the record, and
  `seg:<nonce>:<E>:*` records present ends as `Aborting@E+1`. One batch installs `retire:bytes`
  naming the session and its parts **and** `retire:records` naming exactly `seg:<nonce>:<E>`
  (`0016:665`). Both decode through `decode_retire_obligation`, and the records obligation's
  `segments()` names that group. That draining empties the range is #659's to prove.
  **(H) What cannot be fenced cleanly is never passed off as done.** (i) A `Completing` record
  with **no** nonce — the pre-decision shape — fails decode; restore leaves it byte-identical
  (ADR-0045) and names it as needing a human. (ii) A `Completing` session whose `seg:` records
  name a chunk none of its `part:` records holds is still fenced, and still named as needing a
  human. In both, `RestoreReport::needs_human()` is true (`restore.rs:212`).
  **(H-iii) An untrusted staged record is reported, and needs no human** — the `restore.rs:819`
  question, decided by the human at #664's plan revision (2026-09-18). An untrusted (held)
  record is one the pass read but cannot trust about where its chunk's fragments are: a staged
  placement whose length is not its scheme's fragment count, or an owned `sidx:` value that
  will not decode under a key that still names its chunk (`gc.rs:656-659`). Fixture: two
  sessions, both fenced by this pass — session 1 holds one `part:` record with a wrong-length
  placement, session 2 holds only trusted records. After the pass:
  (a) the report **names** session 1's record by key: its `Debug` rendering contains
  `staged_untrusted` and that key (as `object_name` renders it — an ASCII key is unchanged,
  `gc.rs:919`), and none of session 2's keys appear there. This is the discriminating arm;
  (b) `needs_human()` is **false**, and `is_clean()` is **false** — the not-a-clean-bill
  predicate already counts findings that need no human (marks, under-replication,
  `restore.rs:186-199`);
  (c) no fragment of that chunk is marked `orphan:`, and the audit line still fires
  (`emit_untrusted_staged`, `restore.rs:1004`).
  Operator text: `restore_verdict` names these records on an **informational** line — not a
  `NEEDS-HUMAN` one — using `named_records` (`cli.rs:1389`) as the unreadable-records line does.
  The line says every fragment of those chunks was kept, and must **not** promise automatic
  cleanup: whether the retire drain removes such a record is #659's, not yet decided. Cover it
  in `cli.rs`'s own report tests (green-only). Replace the `deferred: #664` marker with a pointer
  to this leg. Why no human: once this pass fences the session its bytes are garbage whatever
  the record says, so what is left is cleanup — automatic work, not a judgement. Why not
  silent: a damaged record points at a bug or corruption, and after #808 it blocks every drain
  in the cluster (`desired_state.rs:234-246`), so the operator should hear about it at restore
  time rather than when a drain stalls.
  **(J) The session record carries the nonce, and nothing else changes for it.** A `Completing`
  record with the nonce round-trips byte-identically through its codec, and one without is
  refused. Put this in the codec's own test module in `multipart.rs`; green-only, which is fine
  for a codec leg.
  **(K) A second pass is idempotent.** Re-running `reconcile_after_restore` over the fenced
  store installs no second obligation and fences nothing again, and a case H session is
  **still** named as needing a human on that second pass. This is the half of iteration 1's bug
  that lives in this child: never let "already `Aborting`" mean "nothing to report".
  **(L) `cargo xtask ci` green.**
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the nonce on the `Completing` session record and its codec; the restore fence in
  both shapes, one batch each; `staged_skipped`, `sessions_fenced` and `staged_untrusted` on
  `RestoreReport`, with `is_clean()` counting the last (leg H-iii), replacing the `restore.rs:819`
  marker; `restore_verdict` and the operator paragraphs in
  `crates/server/src/cli.rs:1230-1370`, with its report-literal tests (`:2905-2990`); the fence
  paragraphs in `06-runtime-view.md` and the nonce in `05-building-block-view.md:202` and the
  m4 blueprint's restore steps. `gc.rs` only to widen the paging helpers' visibility if the
  fence reuses them. / out of scope: **any durable record that the fence ran, and any
  "generation"** (child-3 — do not add `mpufence` or the like here); drain status and rebalance
  (child-1); `scrub.rs`, `reconstruction.rs` (#663); the mark codec (#804); the retire drain
  (#659); Abort and Complete (#656, #658); the gateway (#508); `crates/dst/tests/custodian.rs`
  unless an existing case stops passing, and then say so in `build-notes.md`; any edit to 0016
  or an ADR.

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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 95.9% — 378 of 394 instrumentable changed lines executed (floor 80%); 394 of 903 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): unverifiable — gate exceeded its 7200s timeout

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_809/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.12s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Two implementation findings remain in #809’s restore-session fence, which must prevent restored multipart uploads from publishing and account for their staged data; this review is advisory.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Restore safety, required nonce, reporting and repeat-pass behavior have falsifiable criteria; the nonce and untrusted-record policy are settled decisions (`brief.md:12`, `brief.md:33`, `brief.md:103`). |
| C2 Reproduction (red pre-fix) | PASS | Stashing the tracked fix while retaining the new test produced 11 assertion failures after a successful compilation (`reviewer-red.log:6`, `reviewer-red.log:95`). |
| C3 Change | PASS | The change stays on the specified codec, restore, report and living-document surfaces; companion fixture updates accommodate the required nonce (`crates/core/src/multipart.rs:1977`, `crates/custodian/src/restore.rs:1063`, `crates/server/src/cli.rs:1371`, `brief.md:112`). |
| C4 Verification (red→green) | PASS | Restoring the identical patch produces 11/11 passing tests; frozen CI records success, while the independent CI run’s cargo-deny failure is a read-only host-cache fault (`reviewer-green-restored.log:19`, `gate-logs/C4-ci.log:3744`, `reviewer-ci.log:3043`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Make the fence cover large sparse restored part sets—the oversized obligation leaves a decodable Completing session unfenced, so the stated invariant remains false for that case (finding 1; `crates/custodian/src/restore.rs:1075`, `crates/custodian/src/restore.rs:1123`). |
| T1 Structure | PASS | Atomicity belongs to the existing MetadataStore seam: exact session bytes and absent obligation keys precondition the same batch; commit errors propagate (`crates/custodian/src/restore.rs:1117`, `crates/custodian/src/restore.rs:1125`). |
| T2 Shape | PASS | Required nonce validation, canonical round trips and current operator documentation preserve the intended persisted-record boundary (`crates/core/src/multipart.rs:1996`, `crates/core/tests/multipart_session_records.rs:854`, `docs/design/architecture/05-building-block-view.md:202`). |
| T3 Runtime | FAIL | A format-valid 10,000-part sparse set encodes to 128,916 bytes against the 100,000-byte ceiling; the capped-store reproduction returns an error with the session unchanged and zero retirement records (finding 1; `crates/custodian/src/restore.rs:1123`, `crates/core/src/metadata.rs:549`, `reviewer-sparse-restore.log:12`). |
| T4 Contribution | FAIL | The frozen batch review has five unresolved entries, deduplicated here into two grounded findings; the contribution-artifact audit is N/A until its mandatory publish rerun (`gate-logs/T4-batch-review.log:10`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Establish the new fence’s schedule and failure behavior in seeded Tier-0 simulation—the scripted Tokio race and fixture-only DST edit do not satisfy the standing test-fidelity rule (finding 2; `AGENTS.md:188`, `crates/custodian/tests/restore_session_fence.rs:1071`, `crates/dst/tests/custodian.rs:2915`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the restore workflow is operationally sufficient after the findings are resolved—a live restored FDB/fleet topology was not exercised, so recovery evidence rests on compiled production code over doubles; writers must remain stopped during the pass (`brief.md:136`, `docs/design/architecture/m4-first-deployment-blueprint.md:581`). |

1. **The Completing fence can exceed the backend’s value limit.** `read.part_set()` feeds every part-number run into one retirement value, which `commit_fence` writes without a size bound (`crates/custodian/src/restore.rs:1075`, `crates/custodian/src/restore.rs:1123`). The production constructor and codec accept 10,000 singleton runs for part numbers 1, 3, …, 19,999 and produce **128,916 bytes** (`reviewer-sparse-size.log:1`). These numbers are inside the persisted format’s 1–999,999 range (`crates/core/src/multipart.rs:920`); the witness concerns restored format-valid records, and does not establish that a current S3 front door admits them. The backend ceiling is 100,000 bytes (`crates/core/src/metadata.rs:546`).

   An independent scratch copy of the integration harness adds that ceiling to its metadata double, seeds the sparse session, and calls the unchanged production `reconcile_after_restore`. It fails with `value_too_large`, leaves the Completing record byte-identical and installs no obligations (`reviewer-sparse-restore.log:12`). This is a bounded-double reproduction, not a live FDB run. The original double accepts arbitrary value sizes (`crates/custodian/tests/restore_session_fence.rs:175`), and the supplied Completing test uses three contiguous parts (`crates/custodian/tests/restore_session_fence.rs:896`), so its green result cannot catch this failure. Keep the fence and complete teardown coverage atomic while making the encoded obligation fit the backend envelope, and add a sparse worst-case regression. Merely refusing the oversized value still leaves the required fence undone.

2. **The new concurrent fence lacks seeded Tier-0 coverage.** The rubric requires it for new destructive or concurrent paths (`AGENTS.md:188`). The added test scripts a single competing commit immediately before the restore commit (`crates/custodian/tests/restore_session_fence.rs:1080`). The changed DST fixture belongs to a driver with only GC and Reconstruction variants (`crates/dst/tests/custodian.rs:2617`, `crates/dst/tests/custodian.rs:3026`). Existing restore DST invokes the pass against ordinary inode publication/corruption, with no multipart session fixture (`crates/dst/tests/custodian.rs:1954`, `crates/dst/tests/custodian.rs:2006`). I reran `cargo xtask dst`; it passes, including all 28 custodian tests (`reviewer-dst.log:511`), without covering the new fence. Add seeded tests that actually invoke this fence against concurrent session/part publication and commit-failure or ambiguous-outcome schedules, asserting fence/obligation atomicity and safe retry. A separate DST test file can avoid modifying the existing file excluded by `brief.md:123`.

The verification evidence has these limits:

- **Independent red→green confirmed.** `git stash` retained the untracked regression file; all 11 tests ran red, then all 11 ran green after `git stash pop`. Binary diffs before and after restoration match. Source citations above ground in the patched disposable `$PDCA_TARGET`; no stale-target fallback was needed.
- **CI largely rerun; host caveat isolated.** Typos, docs lint/render, formatting, workspace clippy/build/tests and cargo-machete passed independently. Cargo-deny stopped at its shared advisory database’s read-only lock (`reviewer-ci.log:3043`); its three actual successful audits are present in the frozen log (`gate-logs/C4-ci.log:3136`, `gate-logs/C4-ci.log:3147`, `gate-logs/C4-ci.log:3150`). Conformance, statics and the full DST command subsequently passed independently (`reviewer-conformance.log:1`, `reviewer-statics.log:3`, `reviewer-dst.log:541`). The TiKV crate and server feature clippy commands also passed independently (`reviewer-tikv.log`). Both named external dependencies, typos and docs-renderer, were exercised (`reviewer-ci.log:2`, `reviewer-ci.log:10`).
- **Frozen wrapper evidence read.** Diff coverage reports 378/394 instrumentable changed lines, 95.9%, with 657 tests (`gate-logs/C4-diff-cov.log:1246`). The instance-scoped coverage/review wrappers were not rerun from the target. Mutation testing found 64 candidates and then timed out after 7,200 seconds; its log provides no surviving/killed-mutant verdict (`gate-logs/C5-mutants.log:7`, `gate-logs/C5-mutants.log:10`). That timeout establishes neither a patch defect nor successful mutation coverage. The batch review’s two size reports and three DST reports reduce to findings 1 and 2.
- **Contribution audit: N/A.** Its subject is intentionally absent at Check, and the log explicitly owes the substantive audit to publish (`gate-logs/T4-contribution.log:10`). There is no missing-evidence escalation.

Prior art was checked by all **15 affected paths**, using fully paginated merged history plus all **355 closed PRs** and the changed-file lists of all **19 unmerged PRs** (`reviewer-prior-art.log:2`). The only rejected path overlaps were #647’s segmented-map work and #336’s DST-state ADR, neither a restore-session fence. The separately rejected #637/#664 iterations described in `brief.md:150` concern the missing segment deleter and disappearing second-pass findings; the independently passing G/K tests address those cases (`crates/custodian/tests/restore_session_fence.rs:919`, `crates/custodian/tests/restore_session_fence.rs:1312`). No duplicate merged fence was found. The explicit drain-order deferral remains settled under #659 (`crates/custodian/src/restore.rs:1265`). No capability probe or load-time symptom guard was added.

For operational validation, use a disposable restored deployment with the full fleet and writers stopped, then run the documented command at `docs/design/architecture/m4-first-deployment-blueprint.md:590` twice. Confirm the first run fences seeded Open/Completing sessions and names damaged ones; the second reports zero new fences while preserving unresolved findings. Confirm staged-untrusted records are informational and their fragments remain kept. The production entry point and report behavior were exercised here through the integration tests; deployment topology, downstream retirement draining (#659) and the durable readiness record (#810) remain outside this slice’s demonstrated behavior.

### Advisory — adversary

# Adversarial review — #809 restore-session-fence

Advisory only. Line numbers are on the patched target (`$PDCA_TARGET`).

## Refutations that landed

- NEEDS-HUMAN [human] — **One sparse `Completing` session makes every restore run fail, and leaves every session after it unfenced.** `crates/custodian/src/restore.rs:1075` builds `retire:bytes:{session, parts: <set>}` from the session's whole part set and commits it at `:1125` with no size check. For 10,000 odd-numbered parts (1, 3, …, 19999) the encoded value is **128,916 bytes**, over `MAX_VALUE_BYTES` = 100,000 (`crates/core/src/metadata.rs:549`). It crosses the limit at 7,935 such parts. FDB refuses the value with the permanent `2103 value_too_large`, and `crates/metadata-fdb/src/lib.rs:303` surfaces that as `Err`. That `Err` goes up through `fence_sessions` (`restore.rs:1013`) and `reconcile_after_restore` (`:647`). I reproduced it in a scratch copy with a store double that refuses values over the limit, as FDB does. Both runs returned `Err("2103 value_too_large: retire:bytes:s:0a…0a:3 is 128916 bytes")`, and a second, `Open` session whose key sorts after it stayed `Open@3` on both runs. So the D-B hazard this patch exists to close stays open for every session listed after the bad one. The design text is wrong too: 0016 says "10,000 alternating numbers — stays inside one value" (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:384`). The fence is the first writer to hit that limit. A human has to pick the fix. Option (a): write `{session, all}` for a `Completing` session as well. That is the same key and a shape decode already accepts (`open_teardown`, `multipart.rs:3334`), and it is equivalent because no part can commit once the session leaves `Open`, but it departs from `0016:665`'s `{session, parts}`. Option (b): keep the set, pre-check it with `metadata::flat_value_ceiling_crossed` (`metadata.rs:602`), and name the session unsettled instead of failing the pass. The T4 batch review flagged the size overflow. It did not mention that the failure repeats on every run or that later sessions go unfenced.

- NEEDS-HUMAN [impl] — **Any fence fault now hides the DANGLING and MISPLACED verdicts, which is the #651 defect class again.** The fence runs at `restore.rs:647`, before Pass 3 (`:658-705`), which is where `emit_dangling` (`:692`) and `emit_misplaced` (`:699`) fire. On the base, once the marks had landed, nothing between them and Pass 3 read the store, so the verdicts were always printed. Now any fence `Err`, transient or permanent (the case above), returns before a single dangling or misplaced chunk is reported. That is the "no report AT ALL" outcome the file's own comment at `:429-434` says #651 fixed, and it breaks the file's rule at `:446-452` that findings are reported before the next store read. Pass 3 reads nothing from the store, and the fence needs nothing Pass 3 computes. Running the fence after Pass 3, just before `emit_summary`, costs nothing and keeps those audit lines when the fence fails. Concrete case: a store holding the sparse session above plus one dangling chunk. The base names the chunk DANGLING. The patched pass never does, on any run.

- NEEDS-HUMAN [impl] — **The only check on an unreadable segment record has no test.** Diff coverage lists `restore.rs:1231` and `:1241-1244` as never executed (`gate-logs/C4-diff-cov.log`: MISS lines 1206–1244), and C5 mutants timed out, so nothing pins these branches. For a `Completing` attempt's `seg:` range, `read_attempt` is the **only** reader in the restore pass: the staged read covers `sidx:` and `part:` only, and `referenced_fragments` covers only groups a committed inode has adopted. Concrete failing case: delete the `read.unreadable.push` at `:1241`. A `Completing` session whose one `seg:<nonce>:<E>:0` value will not decode is then fenced and reported clean (`needs_human()` false). Meanwhile its `retire:records:{seg}` obligation will delete that record without marking anything, so whatever chunks it named are left with no deleter. That is the X57 / H(ii) property, and all 11 tests still pass. Add an H leg with an undecodable `seg:` value, and one with a stray key under the group's range (`:1231`).

- NEEDS-HUMAN [human] — **The DST rule and the brief's scope conflict.** The rubric's test-fidelity rule says "a new destructive or concurrent path lands with seeded Tier-0 DST coverage". The fence is new, it races a Complete fence and a root flip on the session record, and it installs obligations that lead to deletes. Its only coverage is the scripted in-memory race in `crates/custodian/tests/restore_session_fence.rs:1072`. The one DST change updates a fixture (`crates/dst/tests/custodian.rs:2913`, inside `staged_handoffs_under`) and never runs the fence. The brief put `crates/dst/tests/custodian.rs` out of scope. A human should decide whether to add a seeded DST case here or record a deferral with an issue number. The T4 gate raised the same point three times as blocking.

## Refutation attempts that failed

- **Red→green evidence holds.** `gate-logs/C4-verify.log` shows all 11 new tests red on the base by assertion, with no compile failure, each for the missing behaviour: `(Open, 3)` vs `(Aborting, 4)`, no `staged_skipped`, `unknown field segment_nonce`, and so on. I re-ran the file on a scratch copy of the patched tree: 11/11 green. The tests call the production `reconcile_after_restore`, not a copy of it.
- **The single-batch test is not a tautology.** `f_the_open_fence_lands_whole_or_not_at_all` fails the session put in one run and the obligation put in the other. A fence split into two commits, in either order, leaves a `retire:` key or a changed session record in one of the two runs.
- **Idempotency (leg K) is checked on the whole store** (`snapshot() == after_first`), not by counting, and the stray-segment session is named again on the second pass.
- **The recheck's epoch lookup is sound.** A rollback files `retire:records` at the epoch it leaves and moves the session to `Open@E+1` (`0016:2196`). So the obligation at `E'-1` that the recheck reads for an `Aborting@E'` session can only belong to a Completing→Aborting fence.
- **Commit outcomes.** An `Err` (unknown outcome) is returned to the caller, not read as `Conflict`, which meets the rubric's transaction rule. `Conflict` writes nothing and names the session.
- **Serialization identity.** `fence_to_aborting` output re-encodes byte-identically to the `Aborting@E+1` witness (leg J). The nonce decodes through `SegmentNonce::new`, so uppercase or short spellings are refused.
- **Existing tests were not weakened.** The `staged_protection.rs` session-value leg now also requires the session to be named `session-unsettled` and not `unresolvable-staged-record`. The other fixture edits only add the nonce.
- **The CLI summary's arguments match its placeholders in order** (`crates/server/src/cli.rs`, `restore_verdict`).

### Advisory — code-review

- NEEDS-HUMAN [impl] — **Oversized retirement values leave sparse `Completing` sessions unfenced.** `crates/custodian/src/restore.rs:1075` collects every part into one obligation, and `crates/custodian/src/restore.rs:1123` writes its encoding without a size bound. For 10,000 parts numbered 1, 3, …, 19,999 (within the stored part-number grammar), the resulting `{"session":true,"parts":[[1,1],[3,3],…]}` occupies **128,916 bytes**, exceeding `MAX_VALUE_BYTES = 100_000` (`crates/core/src/metadata.rs:549`). FoundationDB therefore rejects the entire batch: the session remains `Completing`, neither obligation lands, and every retry fails identically, also preventing later sessions from being fenced. Keep the fence and all retirement obligations atomic while using a representation that fits the backend limit; add a sparse-part regression with a size-enforcing store double. The new double currently accepts arbitrary value sizes (`crates/custodian/tests/restore_session_fence.rs:175`), so the green tests miss this case.

No additional reuse, simplification, or efficiency findings. Reviewed the frozen gate evidence and target source; independently checked the compact JSON size without modifying the target or rerunning builds.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Make the fence cover large sparse restored part sets—the oversized obligation leaves a decodable Completing session unfenced, so the stated invariant remains false for that case (finding 1; `crates/custodian/src/restore.rs:1075`, `crates/custodian/src/restore.rs:1123`).
- [ ] T5 Judgment — Establish the new fence’s schedule and failure behavior in seeded Tier-0 simulation—the scripted Tokio race and fixture-only DST edit do not satisfy the standing test-fidelity rule (finding 2; `AGENTS.md:188`, `crates/custodian/tests/restore_session_fence.rs:1071`, `crates/dst/tests/custodian.rs:2915`).
- [ ] Validation — fitness-to-purpose — Decide whether the restore workflow is operationally sufficient after the findings are resolved—a live restored FDB/fleet topology was not exercised, so recovery evidence rests on compiled production code over doubles; writers must remain stopped during the pass (`brief.md:136`, `docs/design/architecture/m4-first-deployment-blueprint.md:581`).
- [ ] **One sparse `Completing` session makes every restore run fail, and leaves every session after it unfenced.** `crates/custodian/src/restore.rs:1075` builds `retire:bytes:{session, parts: <set>}` from the session's whole part set and commits it at `:1125` with no size check. For 10,000 odd-numbered parts (1, 3, …, 19999) the encoded value is **128,916 bytes**, over `MAX_VALUE_BYTES` = 100,000 (`crates/core/src/metadata.rs:549`). It crosses the limit at 7,935 such parts. FDB refuses the value with the permanent `2103 value_too_large`, and `crates/metadata-fdb/src/lib.rs:303` surfaces that as `Err`. That `Err` goes up through `fence_sessions` (`restore.rs:1013`) and `reconcile_after_restore` (`:647`). I reproduced it in a scratch copy with a store double that refuses values over the limit, as FDB does. Both runs returned `Err("2103 value_too_large: retire:bytes:s:0a…0a:3 is 128916 bytes")`, and a second, `Open` session whose key sorts after it stayed `Open@3` on both runs. So the D-B hazard this patch exists to close stays open for every session listed after the bad one. The design text is wrong too: 0016 says "10,000 alternating numbers — stays inside one value" (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:384`). The fence is the first writer to hit that limit. A human has to pick the fix. Option (a): write `{session, all}` for a `Completing` session as well. That is the same key and a shape decode already accepts (`open_teardown`, `multipart.rs:3334`), and it is equivalent because no part can commit once the session leaves `Open`, but it departs from `0016:665`'s `{session, parts}`. Option (b): keep the set, pre-check it with `metadata::flat_value_ceiling_crossed` (`metadata.rs:602`), and name the session unsettled instead of failing the pass. The T4 batch review flagged the size overflow. It did not mention that the failure repeats on every run or that later sessions go unfenced.
- [ ] **Any fence fault now hides the DANGLING and MISPLACED verdicts, which is the #651 defect class again.** The fence runs at `restore.rs:647`, before Pass 3 (`:658-705`), which is where `emit_dangling` (`:692`) and `emit_misplaced` (`:699`) fire. On the base, once the marks had landed, nothing between them and Pass 3 read the store, so the verdicts were always printed. Now any fence `Err`, transient or permanent (the case above), returns before a single dangling or misplaced chunk is reported. That is the "no report AT ALL" outcome the file's own comment at `:429-434` says #651 fixed, and it breaks the file's rule at `:446-452` that findings are reported before the next store read. Pass 3 reads nothing from the store, and the fence needs nothing Pass 3 computes. Running the fence after Pass 3, just before `emit_summary`, costs nothing and keeps those audit lines when the fence fails. Concrete case: a store holding the sparse session above plus one dangling chunk. The base names the chunk DANGLING. The patched pass never does, on any run.
- [ ] **The only check on an unreadable segment record has no test.** Diff coverage lists `restore.rs:1231` and `:1241-1244` as never executed (`gate-logs/C4-diff-cov.log`: MISS lines 1206–1244), and C5 mutants timed out, so nothing pins these branches. For a `Completing` attempt's `seg:` range, `read_attempt` is the **only** reader in the restore pass: the staged read covers `sidx:` and `part:` only, and `referenced_fragments` covers only groups a committed inode has adopted. Concrete failing case: delete the `read.unreadable.push` at `:1241`. A `Completing` session whose one `seg:<nonce>:<E>:0` value will not decode is then fenced and reported clean (`needs_human()` false). Meanwhile its `retire:records:{seg}` obligation will delete that record without marking anything, so whatever chunks it named are left with no deleter. That is the X57 / H(ii) property, and all 11 tests still pass. Add an H leg with an undecodable `seg:` value, and one with a stray key under the group's range (`:1231`).
- [ ] **The DST rule and the brief's scope conflict.** The rubric's test-fidelity rule says "a new destructive or concurrent path lands with seeded Tier-0 DST coverage". The fence is new, it races a Complete fence and a root flip on the session record, and it installs obligations that lead to deletes. Its only coverage is the scripted in-memory race in `crates/custodian/tests/restore_session_fence.rs:1072`. The one DST change updates a fixture (`crates/dst/tests/custodian.rs:2913`, inside `staged_handoffs_under`) and never runs the fence. The brief put `crates/dst/tests/custodian.rs` out of scope. A human should decide whether to add a seeded DST case here or record a deferral with an issue number. The T4 gate raised the same point three times as blocking.
- [ ] **Oversized retirement values leave sparse `Completing` sessions unfenced.** `crates/custodian/src/restore.rs:1075` collects every part into one obligation, and `crates/custodian/src/restore.rs:1123` writes its encoding without a size bound. For 10,000 parts numbered 1, 3, …, 19,999 (within the stored part-number grammar), the resulting `{"session":true,"parts":[[1,1],[3,3],…]}` occupies **128,916 bytes**, exceeding `MAX_VALUE_BYTES = 100_000` (`crates/core/src/metadata.rs:549`). FoundationDB therefore rejects the entire batch: the session remains `Completing`, neither obligation lands, and every retry fails identically, also preventing later sessions from being fenced. Keep the fence and all retirement obligations atomic while using a representation that fits the backend limit; add a sparse-part regression with a size-enforcing store double. The new double currently accepts arbitrary value sizes (`crates/custodian/tests/restore_session_fence.rs:175`), so the green tests miss this case.
- [ ] C5 surviving mutants on the bundle diff (cargo mutants --in-diff) unverifiable — gate exceeded its 7200s timeout
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_809/review-b
- [ ] size backstop — this slice is behaving oversized: patch is 179 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Slice too big: patch 179 KB (size backstop, threshold 100 KB) and C5 mutation testing timed out at 7200s on 64 mutants with no verdict. Split at re-plan (`pdca split 809`); how to split is left to the re-plan. Carry into whichever child owns the Completing-session fence: - Size bug: `retire:bytes {session, parts}` for a sparse Completing session (~7,935+ non-adjacent parts) exceeds FDB's 100,000-byte value limit, so every restore run fails and later sessions stay unfenced. Fix options for the re-plan to pick: (a) `{session, all}` as for Open, or (b) pre-check with `flat_value_ceiling_crossed` and name the session unsettled. Add a sparse regression with a size-enforcing store double. - Run the fence after Pass 3 (DANGLING/MISPLACED) so a fence error cannot hide those verdicts (#651 class). - Test the unreadable / stray `seg:` record branch in `read_attempt` (uncovered in diff coverage). Open for the re-plan: seeded Tier-0 DST coverage for the fence (rubric requires it; the brief put `crates/dst/tests/custodian.rs` out of scope). Add it in a new DST file or record a deferral with an issue number.
- By / date: Eduard Ralph / 2026-09-29

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_809: C5-mutants timed out at 7200s with 64 mutants and no verdict — the human reads this as a sign the slice is too big; consider using a mutants timeout (or mutant count) as a size signal.
