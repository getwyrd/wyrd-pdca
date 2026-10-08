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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 36 mutants tested in 3m: 8 caught, 28 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_842/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.10s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review: restore must fence resurrected `Completing` uploads atomically with their bytes and segment-record deleters, while preserving actionable warnings on reruns.

No implementation defect was found in this review. The specified regression suite independently reproduces seven assertion failures before the fix and seven passes afterward. Human sign-off still owes the prior-art confirmation and operational fitness decision below.

Source citations resolve inside `$PDCA_TARGET` (`target/`); evidence citations resolve beside this report.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The approved `{session, all}` decision removes the sparse-payload ceiling failure while retaining explicit atomicity, collision and rerun criteria; no new scope decision is needed (`brief.md:25`, `brief.md:36`, `brief.md:93`). |
| C2 Reproduction (red pre-fix) | PASS | Stashing tracked changes while retaining the new test produced seven assertion failures, not compilation failures; the pre-fix sessions remained unfenced (`reviewer-evidence/red-green.log:148`, `reviewer-evidence/red-green.log:213`). |
| C3 Change | PASS | The seven-file change covers the authorized transition, diagnostics, regression cases and living documentation; it totals 81,746 bytes (79.83 KiB), and reverse-application confirms the supplied target matches the patch (`reviewer-evidence/scope-and-grounding.log:1`, `docs/design/architecture/06-runtime-view.md:65`). |
| C4 Verification (red→green) | PASS | Restoring the patch makes all seven regressions pass; ordinary workspace tests also passed serially, and frozen evidence establishes full CI success; independent full-CI completion and diff coverage have the caveats below (`reviewer-evidence/red-green.log:251`, `gate-logs/C4-ci.log:3917`). |
| C5 Causal adequacy | PASS | Atomic fencing with both absence preconditions prevents publication without silently replacing a deleter; reruns validate the retained nonce and report absent or foreign obligations, addressing the cause without a capability fallback (`crates/custodian/src/restore.rs:824`, `crates/custodian/src/restore.rs:911`, `crates/custodian/src/restore.rs:949`). |
| T1 Structure | PASS | Validated transition construction stays in core and storage effects stay behind `MetadataStore`, preserving dependency direction and the existing atomic batch seam (`crates/core/src/multipart.rs:2291`, `crates/custodian/src/restore.rs:801`). |
| T2 Shape | PASS | Epoch overflow, undecodable records and occupied obligation keys remain explicit failures to fence; canonical payload/key checks and updated operator documentation preserve the standing conventions (`crates/core/src/multipart.rs:5197`, `crates/custodian/tests/restore_completing_fence.rs:444`, `docs/design/architecture/m4-first-deployment-blueprint.md:633`). |
| T3 Runtime | PASS | Paging is exercised beyond 512 segments and through 10,000 sparse parts; failure injection leaves all three fence writes absent and preserves the earlier restore verdicts (`crates/custodian/tests/restore_completing_fence.rs:417`, `crates/custodian/tests/restore_completing_fence.rs:483`, `crates/custodian/tests/restore_completing_fence.rs:545`, `crates/custodian/tests/restore_completing_fence.rs:673`). |
| T4 Contribution | N/A | Contribution artifacts are intentionally absent at Check; their substantive audit must rerun at publish, so this deferred row is neither verified nor a human finding (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Confirm that the recorded path-based prior-art review covers merged and closed/rejected work for all affected paths — the brief names two paths and rejected iterations, but this snapshot has only one synthetic commit and no remote to substantiate the complete check (`brief.md:145`, `reviewer-evidence/scope-and-grounding.log:23`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the stopped-writer restore procedure and first-fault diagnostics as sufficient for operator recovery — execution here covers the production reconciler over in-memory doubles, not a restored live fleet (`crates/custodian/tests/restore_completing_fence.rs:165`, `docs/design/architecture/06-runtime-view.md:65`, `docs/design/architecture/m4-first-deployment-blueprint.md:590`). |

The independent evidence supports the specified behavior, with these limits:

- **CI and dependencies:** `typos`, docs lint/render/link checks, format, clippy, build, ordinary workspace tests and dependency-usage scanning ran successfully. The parallel run stalled in `custodian_day_one`; its serial probe passed all 15 tests. The serial CI run then stopped because `cargo deny` could not lock its read-only advisory database. This is a host restriction, not a patch failure; the frozen log records successful deny checks, subsequent guards and madsim checks (`reviewer-evidence/ci-serial.log:4`, `reviewer-evidence/ci-serial-probe.log:74`, `reviewer-evidence/ci-serial.log:2739`, `gate-logs/C4-ci.log:3296`, `gate-logs/C4-ci.log:3917`). The brief's two named external dependencies were actually exercised.
- **Mutation evidence:** With denied lints disabled, the independent core/custodian rerun covered 32 distinct candidates: **24 caught, eight unviable, zero missed**. Its baseline passed; after the first run reached its time limit, both remaining audit-emitter mutations were caught separately. The all-diff attempt exceeded its baseline timeout before testing mutations, so the four server candidates retain the frozen evidence. That frozen gate reports eight caught and 28 unviable across all 36; unviable candidates are not behavioral proof (`reviewer-evidence/mutation-summary.log:1`, `reviewer-evidence/mutants-focused.log:4`, `reviewer-evidence/mutants-remaining.log:4`, `gate-logs/C5-mutants.log:13`).
- **Diff coverage:** no coverage percentage was produced. The gate tried plain `origin/main`, whereas the brief requires three accepted prerequisite patches; this is a base-state caveat. The supplied composed target compiles, reproduces red→green and passes reverse-application checking (`gate-logs/C4-diff-cov.log:10`, `brief.md:5`, `reviewer-evidence/scope-and-grounding.log:10`).
- **Other frozen gates:** the batch-review log reports zero blocking findings, and the TiKV crate/server feature checks show completed compilations. Their instance-scoped wrappers were adjudicated from their supplied logs (`gate-logs/T4-batch-review.log:10`, `gate-logs/host-tikv.log:110`, `gate-logs/host-tikv.log:209`).

The tracked deferrals remain settled: seeded Tier-0 coverage belongs to #843, and interaction with a partially completed retirement drain belongs to #659 (`crates/custodian/src/restore.rs:785`, `crates/custodian/src/restore.rs:932`). Those follow-ups should cover restore/retirement behavior; this patch introduces no new fragment-reconstruction or device-fault path requiring a separate Tier-1/Tier-2 campaign. No `INTEGRATION.md` was supplied. The target's patch was restored unchanged after the red leg; no implementation edits were made.

### Advisory — adversary

# Adversarial review — #842 (809.4), restore fence for `Completing` sessions

Bottom line: I could not break the main path. The red→green proof holds up, the fence is one atomic
batch, and the mutants that matter are killed. I found one small logic gap in the re-run check and
one stale doc block, both cheap to fix, plus one scope question.

## Findings

- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:957`: the re-run check accepts a records
  obligation that owes the session's own segment group **plus** a part set. It compares only
  `owed.segments()`. Concrete case (my scratch probe, production code unchanged): an `Aborting@4`
  session, `retire:bytes:s:<id>:3` = `{"session":true,"parts":"all"}`, and
  `retire:records:s:<id>:3` = `{"parts":[[1,2]],"seg":{"nonce":"<own nonce>","epoch":3}}`, which
  decodes. With clean segments, the pass returns `segments_unaccounted: []` and
  `needs_human() == false`. The same test file names `{parts}` alone as damage (`fc`,
  `crates/custodian/tests/restore_completing_fence.rs:614`, `NotOfAttempt`). No writer files a
  `{parts}` records obligation for an aborted session: 0016:356 says only the publication batch
  does. Its drain would delete the `part:` records that `{session, all}` has to list at drain
  time, leaving part bytes unmarked. That is the X104 outcome (0016:2633). The doc comment just
  above the check (`restore.rs:928-930`) says "trusted only if it owes `group`: one owing anything
  else is the first record at fault", and the code does not do that. Fix: also require
  `owed.parts().is_none()` in the guard, and add this payload as a fifth `Aborting@4` case in leg
  K. Low severity: it needs a damaged store.

- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:397` and `:421-427`: the rustdoc of the
  public `reconcile_after_restore` still says "every session the image holds `Open` is fenced" and
  describes only the `Open@E` / `{session, all}` commit. The patch added a paragraph at `:432-434`
  but left this heading and summary contradicting it. Related wording in
  `docs/design/architecture/06-runtime-view.md:65`: "no obligation already holds that key" is
  singular, but a `Completing` fence now requires two keys absent ("either key"). Doc-only nit.

- NEEDS-HUMAN [human] — `crates/custodian/src/restore.rs:931`: the patch's own reasoning ("its
  absence proves nothing (a damaged or hand-repaired store)") applies just as much to the **bytes**
  obligation, which the re-run check never reads. Probe: `Aborting@4`, a correct
  `retire:records:s:<id>:3` `{seg}`, **no** `retire:bytes:s:<id>:3`, two live `part:` records.
  Result: clean, `needs_human() == false`, and those parts have no deleter. The brief limits leg K
  to the `seg:` side, and the `Open` fence (child-3) has the same gap, so this is outside this
  PR's scope. Per the rubric it needs a decline with a tracking-issue reference, not an in-PR fix.
  A human should decide whether to file that issue.

## Evidence I re-checked (no refutation)

- Red→green, re-run by me: with base `multipart.rs`, `restore.rs` and `cli.rs` and the new test
  kept, 7 of 7 tests fail by assertion (the build succeeds; base names the session
  `cause: Completing`). With the patch, 7 of 7 pass. Every leg calls the production
  `reconcile_after_restore` (`restore_completing_fence.rs`, `restore_pass`). Nothing in the test
  re-implements production code.
- The C5 gate row ("36 mutants: 8 caught, 28 unviable") is weak evidence by itself, because most
  of the "unviable" mutants only failed to compile under the workspace's deny-lints. I re-ran
  cargo-mutants on the diff with `--cap-lints=true` (lints downgraded, so those mutants compile).
  `restore.rs`: 21 caught, 3 unviable, **0 missed**. `multipart.rs`: 3 caught, 5 unviable
  (no `Default` impl), **0 missed**. I also ran a manual mutant that stops the `seg:` read after
  its first page (`restore.rs:991`, `next.filter(|_| false)`). The `paged` case kills it
  (`restore_completing_fence.rs:545`; H and K both fail).
- The G-sparse premise holds: `{"session":true,"parts":[[1,1],[3,3],…,[19999,19999]]}` encodes to
  exactly 128,916 bytes, more than `MAX_VALUE_BYTES` (100,000). So the test's ceiling double would
  refuse a `{session, parts}` regression, and the pass would return `Err`.
- The C4 diff-coverage "fail" happens because the patch is stacked on child-1..3 and does not
  apply to bare `origin/main`. That is a harness limit, not evidence against the fix.

## Attacks tried, could not refute

- Atomicity: the session, bytes and records writes go in one `WriteBatch`
  (`restore.rs:824-831`), with `require` on the session and `require_absent` on both obligation
  keys. G-atomic fails each of the three keys in turn, and a split-commit mutant would leave a
  partial write that the test catches.
- Collision: when both keys are taken, the bytes key is named first (my probe). Each key is
  pinned on its own by G-collision (`c1`, `c4`).
- False alarms on correct stores: a rollback leaves `Completing@E` for `Open@E+1` and files at `E`
  (0016:2196). So an `Open`-fenced `Aborting@E'` session's range at `E'-1` is always empty, and the
  re-check stays quiet (control `fe`). The reaper's `Completing → Aborting` records shape
  (`{seg:<g>:E}`, 0016:665) matches what the re-check expects.
- Key-grammar edges: `seg:<n>:3:` cannot prefix-match `seg:<n>:30:`, and a 7-digit or suffixed
  index fails `parse_seg_key` and is named `KeyNotOfGroup` (`restore.rs:1002`).
- `u64::MAX`: `completing_teardown` returns `None` through `checked_add`, which leads to
  `EpochExhausted`. No wrap, no panic, and the later `Open` session is still fenced (H(vi)).
- Settled deferrals I did not re-raise: #659 (half-drained ranges, `restore.rs:932-934`) and #843
  (Tier-0 DST, test file header and `restore.rs:785`).

### Advisory — code-review

No findings on either advisory lens: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues found in this diff.

Reviewed the atomic fence and both collision guards (`crates/custodian/src/restore.rs:824`), repeat-pass obligation validation and paging (`crates/custodian/src/restore.rs:935`), teardown construction (`crates/core/src/multipart.rs:2291`), regression coverage (`crates/custodian/tests/restore_completing_fence.rs:394`), and operator reporting (`crates/server/src/cli.rs:1402`). All source citations were checked against `$PDCA_TARGET`.

Validation uses the frozen gate evidence; no builds or tests were rerun. CI passed, all seven new tests failed before the fix and passed after it, and C5 reported 8 caught and 28 unviable mutants with none missed. Diff coverage was not measured because the patch did not apply to `origin/main`. The explicit #659 and #843 deferrals remain settled for this review.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T5 Judgment — Confirm that the recorded path-based prior-art review covers merged and closed/rejected work for all affected paths — the brief names two paths and rejected iterations, but this snapshot has only one synthetic commit and no remote to substantiate the complete check (`brief.md:145`, `reviewer-evidence/scope-and-grounding.log:23`).
- [ ] Validation — fitness-to-purpose — Accept the stopped-writer restore procedure and first-fault diagnostics as sufficient for operator recovery — execution here covers the production reconciler over in-memory doubles, not a restored live fleet (`crates/custodian/tests/restore_completing_fence.rs:165`, `docs/design/architecture/06-runtime-view.md:65`, `docs/design/architecture/m4-first-deployment-blueprint.md:590`).
- [ ] `crates/custodian/src/restore.rs:957`: the re-run check accepts a records obligation that owes the session's own segment group **plus** a part set. It compares only `owed.segments()`. Concrete case (my scratch probe, production code unchanged): an `Aborting@4` session, `retire:bytes:s:<id>:3` = `{"session":true,"parts":"all"}`, and `retire:records:s:<id>:3` = `{"parts":[[1,2]],"seg":{"nonce":"<own nonce>","epoch":3}}`, which decodes. With clean segments, the pass returns `segments_unaccounted: []` and `needs_human() == false`. The same test file names `{parts}` alone as damage (`fc`, `crates/custodian/tests/restore_completing_fence.rs:614`, `NotOfAttempt`). No writer files a `{parts}` records obligation for an aborted session: 0016:356 says only the publication batch does. Its drain would delete the `part:` records that `{session, all}` has to list at drain time, leaving part bytes unmarked. That is the X104 outcome (0016:2633). The doc comment just above the check (`restore.rs:928-930`) says "trusted only if it owes `group`: one owing anything else is the first record at fault", and the code does not do that. Fix: also require `owed.parts().is_none()` in the guard, and add this payload as a fifth `Aborting@4` case in leg K. Low severity: it needs a damaged store.
- [ ] `crates/custodian/src/restore.rs:397` and `:421-427`: the rustdoc of the public `reconcile_after_restore` still says "every session the image holds `Open` is fenced" and describes only the `Open@E` / `{session, all}` commit. The patch added a paragraph at `:432-434` but left this heading and summary contradicting it. Related wording in `docs/design/architecture/06-runtime-view.md:65`: "no obligation already holds that key" is singular, but a `Completing` fence now requires two keys absent ("either key"). Doc-only nit.
- [ ] `crates/custodian/src/restore.rs:931`: the patch's own reasoning ("its absence proves nothing (a damaged or hand-repaired store)") applies just as much to the **bytes** obligation, which the re-run check never reads. Probe: `Aborting@4`, a correct `retire:records:s:<id>:3` `{seg}`, **no** `retire:bytes:s:<id>:3`, two live `part:` records. Result: clean, `needs_human() == false`, and those parts have no deleter. The brief limits leg K to the `seg:` side, and the `Open` fence (child-3) has the same gap, so this is outside this PR's scope. Per the rubric it needs a decline with a tracking-issue reference, not an in-PR fix. A human should decide whether to file that issue.
- [ ] **Atomicity alone does not protect the new retirement obligation from overwrite.** G requires three writes in one commit and a failing-commit witness (`brief.md:35-43`), but never requires a collision witness for the newly added `retire:records:s:<id>:<E>` key. The target explicitly requires `require_absent` on obligation installation and classification of collisions, because overwriting a payload permanently loses reclamation evidence (`crates/core/src/multipart.rs:1409-1414`; `docs/design/proposals/draft/0016-multipart-commit-protocol.md:369-373`). A blind put of the records obligation can satisfy the listed fresh-store and atomicity cases. Revise G to require the absence guard and seed an existing, decodable same-epoch obligation naming a different segment group: its bytes must survive, none of the three fence writes may land, and the unresolved collision must be reported.
- [ ] **The promised epoch transition has an accepted input with no successor.** G says `Completing@E` ends at `Aborting@E+1` (`brief.md:35-38`), and the invariant is unconditional (`brief.md:76-79`). On the supplied target, the session epoch is any `u64` (`crates/core/src/multipart.rs:2085`); Completing validation checks cursor bounds and matching target identity/epoch, but does not reject `u64::MAX` (`crates/core/src/multipart.rs:2204-2240`). Such a decodable record cannot take the required transition. Revise H to cover a Completing record at that boundary: checked increment, no wrap/panic or partial obligations, unchanged session reported as needing a human, and continued fencing of a later eligible session. Explicitly apply any inherited child-3 guard to Completing, and qualify the invariant for sessions left unresolved instead of promising universal fencing.
- [ ] T5 Judgment — Confirm the path-based prior-art disposition against merged history and closed/rejected work — the brief records that check, but this one-commit snapshot has no remote or historical review evidence to corroborate it (`brief.md:145`, `pdca-reviewer-842-evidence/source-audit.log:10`).
- [ ] T5 Judgment — Confirm merged-history and closed/rejected-work coverage by affected path on the publication base: the brief documents restore.rs/multipart.rs prior art, but this target exposes only one synthetic commit and no remote, so broader coverage cannot be independently settled (`brief.md:145`; `reviewer-evidence/target-grounding.txt:2`).
- [ ] T5 Judgment — Confirm merged and closed/rejected prior art for all seven affected paths — the supplied single-commit target has no remote/history to establish whether equivalent work already exists (`reviewer-evidence/prior-art.log:1`, `brief.md:145`).
- [ ] the C5 row ("36 mutants tested: 8 caught, 28 unviable", 0 missed) is not evidence of adequacy. The workspace sets `warnings = "deny"` (`Cargo.toml:230`), and `.cargo/mutants.toml` sets no `cap_lints`. So any mutant that stubs a body and leaves a parameter unused fails to compile, and is counted "unviable". Re-running the same 36 mutants with `--cap-lints=true` gives **25 caught, 2 missed, 9 unviable**; the 2 missed are the ones in bullet 1. Whether the gate (or `.cargo/mutants.toml`) should cap lints is a harness/repo decision outside this diff. The two survivors themselves are bullet 1's [impl] work.
- [ ] leg (L), "`cargo xtask ci` green", is unverified, yet `check-gates.json` reports `overall: "pass"`. The gating C4-ci row is `unverifiable`: it hit the 7200 s timeout inside `crates/server/tests/custodian_day_one.rs`, after fmt, clippy, build and most tests had passed. It never ran machete, deny, conformance, statics, the orchestrator guard or DST. The hang is not this patch: here, with the patch applied, `custodian_day_one` passes 15/15 in 0.19 s. What I re-ran myself, all green: `cargo xtask statics`; `cargo clippy -p wyrd-dst --all-targets` under `--cfg madsim`; and the DST `restore*` tests. Still not run: machete, deny, conformance, the orchestrator guard, the full DST seed sweep, and the server tests after `custodian_day_one`. Provisional (toolchain/time), not a refutation.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) unverifiable — gate exceeded its 7200s timeout

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
- Iteration delta (if iterating): Auto-iterate (round 5): rebuilding for the implementation-level findings — `crates/custodian/src/restore.rs:957`: the re-run check accepts a records obligation that owes the session's own segment group **plus** a part set. It compares only `owed.segments()`. Concrete case (my scratch probe, production code unchanged): an `Aborting@4` session, `retire:bytes:s:<id>:3` = `{"session":true,"parts":"all"}`, and `retire:records:s:<id>:3` = `{"parts":[[1,2]],"seg":{"nonce":"<own nonce>","epoch":3}}`, which decodes. With clean segments, the pass returns `segments_unaccounted: []` and `needs_human() == false`. The same test file names `{parts}` alone as damage (`fc`, `crates/custodian/tests/restore_completing_fence.rs:614`, `NotOfAttempt`). No writer files a `{parts}` records obligation for an aborted session: 0016:356 says only the publication batch does. Its drain would delete the `part:` records that `{session, all}` has to list at drain time, leaving part bytes unmarked. That is the X104 outcome (0016:2633). The doc comment just above the check (`restore.rs:928-930`) says "trusted only if it owes `group`: one owing anything else is the first record at fault", and the code does not do that. Fix: also require `owed.parts().is_none()` in the guard, and add this payload as a fifth `Aborting@4` case in leg K. Low severity: it needs a damaged store.; `crates/custodian/src/restore.rs:397` and `:421-427`: the rustdoc of the public `reconcile_after_restore` still says "every session the image holds `Open` is fenced" and describes only the `Open@E` / `{session, all}` commit. The patch added a paragraph at `:432-434` but left this heading and summary contradicting it. Related wording in `docs/design/architecture/06-runtime-view.md:65`: "no obligation already holds that key" is singular, but a `Completing` fence now requires two keys absent ("either key"). Doc-only nit.. 10 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
