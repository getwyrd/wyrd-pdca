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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 39 mutants tested in 4m: 2 missed, 8 caught, 29 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_842/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.17s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

The #842 patch meets the tested criteria for fencing restored `Completing` uploads with both retirement obligations; prior-art confirmation and operational fitness remain human decisions.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The accepted `{session, all}` decision and explicit collision, corruption, overflow, and rerun criteria define a falsifiable restore-fence scope (`brief.md:25`, `brief.md:33`, `brief.md:114`). |
| C2 Reproduction (red pre-fix) | PASS | With the tracked fix stashed and the new test retained, all seven tests compiled and failed by assertion, reproducing unfenced sessions (`pdca-reviewer-842-evidence/red.log:146`, `pdca-reviewer-842-evidence/red.log:211`; `crates/custodian/tests/restore_completing_fence.rs:306`). |
| C3 Change | PASS | Publication is fenced together with both deletion obligations, preserving existing obligations on conflict; the seven affected files include the required operator documentation (`crates/core/src/multipart.rs:2294`, `crates/custodian/src/restore.rs:837`, `docs/design/architecture/06-runtime-view.md:65`). |
| C4 Verification (red→green) | PASS | Independent restoration made all seven tests pass, and independent `cargo xtask ci` passed; the frozen coverage failure supplies no measurement because it used the unstacked base (`pdca-reviewer-842-evidence/green.log:17`, `pdca-reviewer-842-evidence/ci.log:3837`, `gate-logs/C4-diff-cov.log:10`). |
| C5 Causal adequacy | PASS | The fence closes the publication window, names the segment deleter regardless of cursor, and avoids the oversized part list; this removes the specified causes without a capability-probe workaround (`crates/core/src/multipart.rs:2280`, `crates/custodian/tests/restore_completing_fence.rs:407`, `crates/custodian/tests/restore_completing_fence.rs:495`). |
| T1 Structure | PASS | Validated record construction stays in core, while the custodian uses the existing metadata trait and paging seam; no new concrete-backend dependency or clock is introduced (`crates/core/src/multipart.rs:3669`, `crates/custodian/src/restore.rs:965`). |
| T2 Shape | PASS | Stored obligations decode against their keys, both require absence, and tests check the exact transitioned session bytes and one shared commit (`crates/custodian/src/restore.rs:837`, `crates/custodian/tests/restore_completing_fence.rs:293`, `crates/custodian/tests/restore_completing_fence.rs:314`). |
| T3 Runtime | PASS | Executed tests cover commit failure, collisions, 10,000 sparse parts, damaged segments, epoch exhaustion, and persistent second-pass reporting; the segment walk uses bounded pages (`crates/custodian/tests/restore_completing_fence.rs:430`, `crates/custodian/tests/restore_completing_fence.rs:574`, `crates/custodian/tests/restore_completing_fence.rs:601`, `crates/custodian/src/restore.rs:973`). |
| T4 Contribution | N/A | Contribution artifacts are intentionally drafted after Check; their substantive audit must rerun at publish, as the deferred gate states (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Confirm the path-based prior-art disposition against merged history and closed/rejected work — the brief records that check, but this one-commit snapshot has no remote or historical review evidence to corroborate it (`brief.md:145`, `pdca-reviewer-842-evidence/source-audit.log:10`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the stopped-writer recovery procedure and operator handling of segment anomalies before retirement drains resume — the passing in-memory tests establish metadata behavior, while operational fitness remains a human decision (`docs/design/architecture/m4-first-deployment-blueprint.md:633`, `crates/server/src/cli.rs:1411`). |

Source citations above are relative to `$PDCA_TARGET`; brief and evidence citations are relative to this review directory. The target contains the prerequisite changes and the applied patch: reverse-application checks succeeded, and the tracked diff was byte-identical after the stash/pop cycle (`pdca-reviewer-842-evidence/source-audit.log:18`). No implementation defect was found.

The independent evidence supports the functional verdict:

- `cargo test --offline --locked -p wyrd-custodian --test restore_completing_fence` produced seven assertion failures before the fix and seven passes afterward. Full `cargo xtask ci` then passed, including the existing Open-fence regressions, CLI reporting tests, workspace checks, conformance, and existing DST tests. Both named external tools were actually exercised: `typos` ran, and the docs renderer generated 99 pages and passed its link audit (`pdca-reviewer-842-evidence/ci.log:4`, `pdca-reviewer-842-evidence/ci.log:9`, `pdca-reviewer-842-evidence/ci.log:3837`).
- The frozen mutation gate reported two survivors (`gate-logs/C5-mutants.log:13`). A targeted rerun explicitly selecting the custodian integration suite caught deletion of the `Aborting` assignment (`pdca-reviewer-842-evidence/mutants-rerun.log:5`). The surviving conjunction-to-disjunction mutation at `crates/custodian/src/restore.rs:1001` is equivalent under the production scan contract: `check_attempt` scans the exact nonce-and-epoch prefix, and every successfully parsed key within it necessarily matches both. Malformed keys still take the error arm (`crates/custodian/src/restore.rs:972`, `crates/core/src/metadata.rs:1517`, `crates/core/src/metadata.rs:1539`, `crates/traits/src/lib.rs:1358`). The rerun tested three selected mutations: one caught, one equivalent survivor, one unviable; this does not turn the frozen mutation row green.
- Diff coverage remains unmeasured. Its frozen log only establishes that the patch did not apply to `origin/main`; the brief explicitly requires the preceding children's patches. That is a base-state limitation, not a compilation or correctness defect (`brief.md:6`, `gate-logs/C4-diff-cov.log:10`). The batch-review log records zero blocking findings but contains no underlying review detail, so the source judgments here come from this independent review (`gate-logs/T4-batch-review.log:10`).

The prior-art investigation covered every affected path locally and found only the synthetic base commit, with no remote (`pdca-reviewer-842-evidence/source-audit.log:26`). The brief does record rejected approaches from #809, #637, and #664, and the executed sparse-value, segment-obligation, corruption, and second-pass legs address those specific failures. Independent confirmation of historical completeness is the T5 decision above.

The TiKV feature checks also passed independently: `cargo clippy -p wyrd-metadata-tikv --features tikv --tests` and `cargo clippy -p wyrd-server --features tikv,etcd --tests`, both with `WYRD_TIKV_TOOLCHAIN=1` (`pdca-reviewer-842-evidence/host-tikv.log:105`, `pdca-reviewer-842-evidence/host-tikv.log:206`). This corroborates the successful feature builds in `gate-logs/host-tikv.log:110` and `gate-logs/host-tikv.log:209`.

The existing deferrals remain settled: seeded coverage of this new fence belongs to #843, and interaction with partially drained ranges belongs to #659 (`crates/custodian/src/restore.rs:780`, `crates/custodian/src/restore.rs:924`). This metadata-only change does not warrant an additional Tier-1 disk-fault or Tier-2 kill/reconstruct campaign; no live FoundationDB restore or retirement drain is claimed here.

### Advisory — adversary

# Adversarial review — #842 (809.4) restore fence for `Completing` sessions

**Bottom line: I could not refute the fix.** The red→green proof reproduces, the test drives the
production `reconcile_after_restore`, and every mutant I aimed at a brief leg (G, G-atomic,
G-collision, G-sparse, H, K, Order) was killed. What is left are three **untested branches the
patch adds**, plus one missing core unit test. All four are test gaps the builder can close. None
is a wrong behaviour I could trigger.

## Evidence, re-run independently

- Re-ran in a scratch copy of `$PDCA_TARGET`. **Green:** `restore_completing_fence` 7/7 and
  `restore_open_fence` 9/9. **Red:** with `multipart.rs`, `restore.rs`, `cli.rs` and
  `restore_open_fence.rs` reverted and the new test kept, it builds and 7/7 fail on assertions
  (the same panic sites as `gate-logs/C4-verify.log`). This is not a compile-failure red, and
  the test calls production `reconcile_after_restore` (`crates/custodian/tests/restore_completing_fence.rs:187`),
  not a copy of it.
- Hand mutants **killed** by the new tests: keep the `Completing` state in the teardown
  (`crates/core/src/multipart.rs:2300`); re-check at `epoch` instead of `epoch-1`
  (`crates/custodian/src/restore.rs:933`); skip `check_attempt` after the commit (`restore.rs:851`);
  `require_absent` on the bytes key only, i.e. a blind put of the records obligation (`restore.rs:838`);
  conflict cause read from the first key only (`restore.rs:861`); undecodable segment accepted
  (`restore.rs:1010`); stray key accepted (`restore.rs:1002`).

## Findings

- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:985`: the seg-range paging in
  `check_attempt` is never exercised. Mutant `match (next.filter(|_| false), page.last())`
  (stop after the first page) **survives all 16 fence tests**. Failing case: a `Completing@3`
  session with 513 `seg:` records (`STAGED_PAGE` = 512, `crates/custodian/src/gc.rs:307`) whose
  bad record (undecodable, or naming an unheld chunk) is index 512. The mutant fences it, does
  not name it, and exits 0. The production code pages correctly today, but nothing pins it. Add
  that case to leg H or K.
- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:1028`: the part-range paging in
  `part_chunks` is never exercised either. Mutant "first page of `part:` only" **survives**.
  G-sparse already seeds 10,000 parts, but its two segments name only the chunks of parts 1 and 3
  (`crates/custodian/tests/restore_completing_fence.rs:510-511`), and both sit on page one. Failing
  case for the mutant: point a segment at the chunk of part 19,999. The clean sparse session
  would then be wrongly named `ChunkInNoPart`, and G-sparse's `!names(...)` would go red. This is
  a one-line test change.
- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:952-958`: the `recheck_fenced` arm that
  names an **undecodable `retire:records:s:<id>:<E-1>` obligation** has no test. Replacing it
  with `Err(_) => Ok(())` **survives**. Failing case for the mutant: an `Aborting@4` session
  beside `retire:records:s:<id>:3` = `not json`. Production names it (`SegmentFault::Undecodable`);
  the mutant reports that run as needing no human, and the run still exits 0 (fencing work alone
  is not a failure, per `cli.rs` `restore_verdict`). Also, in this arm the named "record" is the
  obligation key. The CLI paragraph (`crates/server/src/cli.rs` NEEDS-HUMAN text, diff hunk
  @@ -1397) calls every named record a *segment record* ("wrote segment records that include
  one nothing accounts for"), so an operator would go looking under `seg:` for a `retire:` key.
- NEEDS-HUMAN [impl] — `crates/core/src/multipart.rs:2294-2306`: `completing_teardown` has no
  unit test in `wyrd-core`, while `open_teardown` has one (`multipart.rs:5147-5197`). That is why
  C5 reports `multipart.rs:2300:17` (delete `state` field) as MISSED: cargo-mutants runs only
  wyrd-core's own suite for a core file. I confirmed the custodian test kills that mutant (5 of 7
  fail), so this is **not** a behaviour gap. A `mod completing_teardown` twin of
  `mod open_teardown` would make the C5 gate's row reflect that. It should cover the session at
  `Aborting@E+1`, both keys decoding against their payloads, and `None` for
  Open/Aborting/Completed/`u64::MAX`.

## Checked and not raised

- C5's other MISSED mutant (`restore.rs:1001`, `&&`→`||`) is an **equivalent mutant**, not a gap.
  Every key under `seg_range_prefix(group)` = `seg:<nonce>:<E>:` (`crates/core/src/metadata.rs:1517-1519`)
  that `parse_seg_key` accepts has exactly that nonce and canonical epoch (`metadata.rs:1539-1567`),
  so the equality guard can never be false. Only `Ok` vs `Err` matters there.
- `{session, all}` for a `Completing` session: the part set really is frozen, because Part commit
  requires `mpu == Open@E` (0016 rows at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:659`).
  Both payloads are O(1), so G-sparse's ceiling cannot be crossed.
- H(ii) chunk outside every part: the same pass's mark half already treats those fragments as
  unprotected, because the staged class reads only `sidx:`/`part:` (`crates/custodian/src/gc.rs:1560-1575`).
  So bytes leak at worst; nothing is lost.
- A re-check that meets a records obligation naming another session's nonce cannot be produced by
  this fence (`require_absent` on both keys, `restore.rs:838-840`). Only corruption could create
  it. Half-drained ranges are `deferred: #659` (`restore.rs:924-925`), and DST coverage is
  `deferred: #843`. Both are settled under the rubric, so I did not raise them.
- On the `check-gates.json` verdict: `overall: pass` stands, but C4-diff-cov never measured
  anything ("patch.diff does not apply on origin/main", expected for a main+child-1..3 base). The
  three surviving mutants above are exactly the gap that gate would have shown, so the T4 review's
  "0 blocking" says nothing about those branches.

### Advisory — code-review

No findings. I found no introduced correctness bugs or actionable reuse, simplification, or efficiency issues in this diff.

The two frozen mutation survivors do not establish patch defects. Removing the state assignment at `crates/core/src/multipart.rs:2300` contradicts the byte-for-byte `Aborting@E+1` assertion at `crates/custodian/tests/restore_completing_fence.rs:306`, called by the positive test at line 418; the mutation log does not explain why that assertion was not exercised. Changing `&&` to `||` at `crates/custodian/src/restore.rs:1001` is equivalent for well-formed keys returned by the exact group-prefix scan at `crates/custodian/src/restore.rs:972`.

Validation used the frozen gate evidence: CI passed, and all seven new integration tests passed with the fix and failed on the base. Diff coverage was not measured because its origin/main application failed. No builds or tests were rerun, and the target source was left unchanged.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T5 Judgment — Confirm the path-based prior-art disposition against merged history and closed/rejected work — the brief records that check, but this one-commit snapshot has no remote or historical review evidence to corroborate it (`brief.md:145`, `pdca-reviewer-842-evidence/source-audit.log:10`).
- [ ] Validation — fitness-to-purpose — Accept the stopped-writer recovery procedure and operator handling of segment anomalies before retirement drains resume — the passing in-memory tests establish metadata behavior, while operational fitness remains a human decision (`docs/design/architecture/m4-first-deployment-blueprint.md:633`, `crates/server/src/cli.rs:1411`).
- [ ] `crates/custodian/src/restore.rs:985`: the seg-range paging in `check_attempt` is never exercised. Mutant `match (next.filter(|_| false), page.last())` (stop after the first page) **survives all 16 fence tests**. Failing case: a `Completing@3` session with 513 `seg:` records (`STAGED_PAGE` = 512, `crates/custodian/src/gc.rs:307`) whose bad record (undecodable, or naming an unheld chunk) is index 512. The mutant fences it, does not name it, and exits 0. The production code pages correctly today, but nothing pins it. Add that case to leg H or K.
- [ ] `crates/custodian/src/restore.rs:1028`: the part-range paging in `part_chunks` is never exercised either. Mutant "first page of `part:` only" **survives**. G-sparse already seeds 10,000 parts, but its two segments name only the chunks of parts 1 and 3 (`crates/custodian/tests/restore_completing_fence.rs:510-511`), and both sit on page one. Failing case for the mutant: point a segment at the chunk of part 19,999. The clean sparse session would then be wrongly named `ChunkInNoPart`, and G-sparse's `!names(...)` would go red. This is a one-line test change.
- [ ] `crates/custodian/src/restore.rs:952-958`: the `recheck_fenced` arm that names an **undecodable `retire:records:s:<id>:<E-1>` obligation** has no test. Replacing it with `Err(_) => Ok(())` **survives**. Failing case for the mutant: an `Aborting@4` session beside `retire:records:s:<id>:3` = `not json`. Production names it (`SegmentFault::Undecodable`); the mutant reports that run as needing no human, and the run still exits 0 (fencing work alone is not a failure, per `cli.rs` `restore_verdict`). Also, in this arm the named "record" is the obligation key. The CLI paragraph (`crates/server/src/cli.rs` NEEDS-HUMAN text, diff hunk @@ -1397) calls every named record a *segment record* ("wrote segment records that include one nothing accounts for"), so an operator would go looking under `seg:` for a `retire:` key.
- [ ] `crates/core/src/multipart.rs:2294-2306`: `completing_teardown` has no unit test in `wyrd-core`, while `open_teardown` has one (`multipart.rs:5147-5197`). That is why C5 reports `multipart.rs:2300:17` (delete `state` field) as MISSED: cargo-mutants runs only wyrd-core's own suite for a core file. I confirmed the custodian test kills that mutant (5 of 7 fail), so this is **not** a behaviour gap. A `mod completing_teardown` twin of `mod open_teardown` would make the C5 gate's row reflect that. It should cover the session at `Aborting@E+1`, both keys decoding against their payloads, and `None` for Open/Aborting/Completed/`u64::MAX`.
- [ ] **Atomicity alone does not protect the new retirement obligation from overwrite.** G requires three writes in one commit and a failing-commit witness (`brief.md:35-43`), but never requires a collision witness for the newly added `retire:records:s:<id>:<E>` key. The target explicitly requires `require_absent` on obligation installation and classification of collisions, because overwriting a payload permanently loses reclamation evidence (`crates/core/src/multipart.rs:1409-1414`; `docs/design/proposals/draft/0016-multipart-commit-protocol.md:369-373`). A blind put of the records obligation can satisfy the listed fresh-store and atomicity cases. Revise G to require the absence guard and seed an existing, decodable same-epoch obligation naming a different segment group: its bytes must survive, none of the three fence writes may land, and the unresolved collision must be reported.
- [ ] **The promised epoch transition has an accepted input with no successor.** G says `Completing@E` ends at `Aborting@E+1` (`brief.md:35-38`), and the invariant is unconditional (`brief.md:76-79`). On the supplied target, the session epoch is any `u64` (`crates/core/src/multipart.rs:2085`); Completing validation checks cursor bounds and matching target identity/epoch, but does not reject `u64::MAX` (`crates/core/src/multipart.rs:2204-2240`). Such a decodable record cannot take the required transition. Revise H to cover a Completing record at that boundary: checked increment, no wrap/panic or partial obligations, unchanged session reported as needing a human, and continued fencing of a later eligible session. Explicitly apply any inherited child-3 guard to Completing, and qualify the invariant for sessions left unresolved instead of promising universal fencing.

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — `crates/custodian/src/restore.rs:985`: the seg-range paging in `check_attempt` is never exercised. Mutant `match (next.filter(|_| false), page.last())` (stop after the first page) **survives all 16 fence tests**. Failing case: a `Completing@3` session with 513 `seg:` records (`STAGED_PAGE` = 512, `crates/custodian/src/gc.rs:307`) whose bad record (undecodable, or naming an unheld chunk) is index 512. The mutant fences it, does not name it, and exits 0. The production code pages correctly today, but nothing pins it. Add that case to leg H or K.; `crates/custodian/src/restore.rs:1028`: the part-range paging in `part_chunks` is never exercised either. Mutant "first page of `part:` only" **survives**. G-sparse already seeds 10,000 parts, but its two segments name only the chunks of parts 1 and 3 (`crates/custodian/tests/restore_completing_fence.rs:510-511`), and both sit on page one. Failing case for the mutant: point a segment at the chunk of part 19,999. The clean sparse session would then be wrongly named `ChunkInNoPart`, and G-sparse's `!names(...)` would go red. This is a one-line test change.; `crates/custodian/src/restore.rs:952-958`: the `recheck_fenced` arm that names an **undecodable `retire:records:s:<id>:<E-1>` obligation** has no test. Replacing it with `Err(_) => Ok(())` **survives**. Failing case for the mutant: an `Aborting@4` session beside `retire:records:s:<id>:3` = `not json`. Production names it (`SegmentFault::Undecodable`); the mutant reports that run as needing no human, and the run still exits 0 (fencing work alone is not a failure, per `cli.rs` `restore_verdict`). Also, in this arm the named "record" is the obligation key. The CLI paragraph (`crates/server/src/cli.rs` NEEDS-HUMAN text, diff hunk @@ -1397) calls every named record a *segment record* ("wrote segment records that include one nothing accounts for"), so an operator would go looking under `seg:` for a `retire:` key.; `crates/core/src/multipart.rs:2294-2306`: `completing_teardown` has no unit test in `wyrd-core`, while `open_teardown` has one (`multipart.rs:5147-5197`). That is why C5 reports `multipart.rs:2300:17` (delete `state` field) as MISSED: cargo-mutants runs only wyrd-core's own suite for a core file. I confirmed the custodian test kills that mutant (5 of 7 fail), so this is **not** a behaviour gap. A `mod completing_teardown` twin of `mod open_teardown` would make the C5 gate's row reflect that. It should cover the session at `Aborting@E+1`, both keys decoding against their payloads, and `None` for Open/Aborting/Completed/`u64::MAX`.. 3 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
