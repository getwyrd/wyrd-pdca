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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 33 mutants tested in 3m: 7 caught, 26 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_842/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.08s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #842: fence restored `Completing` uploads with atomic byte/segment retirement obligations; one reproducible rerun correctness defect remains.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The accepted `{session, all}` decision, atomicity/collision requirements, damaged-record cases and rerun invariant are explicit and falsifiable (`brief.md:25`, `brief.md:36`, `brief.md:74`). |
| C2 Reproduction (red pre-fix) | PASS | Stashing the tracked fix while retaining the new test produced seven assertion failures, with successful compilation (`reviewer-evidence/red.log:71`; `crates/custodian/tests/restore_completing_fence.rs:403`). |
| C3 Change | PASS | The change stays within the Completing fence, required reporting and living-doc surfaces; the Open-fence regression suite remains green (`brief.md:114`; `crates/custodian/src/restore.rs:891`; `docs/design/architecture/06-runtime-view.md:65`; `reviewer-evidence/green-restored.log:30`). |
| C4 Verification (red→green) | PASS | Restoring the exact patch gives 7/7 new tests and 9/9 Open-fence tests passing; the frozen full-CI log is green, subject to the local-host and unmeasured-coverage caveats below (`reviewer-evidence/green-restored.log:15`, `gate-logs/C4-ci.log:3919`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild the rerun identity check and add its regression: a decodable foreign-group obligation suppresses an existing warning about the real attempt, defeating the “still names what needs a human” guarantee (`crates/custodian/src/restore.rs:946`; `reviewer-evidence/identity.log:28`). |
| T1 Structure | PASS | Record transitions remain in core and maintenance uses the MetadataStore seam; the fence keeps its compare-and-swap and both absence preconditions in one commit (`crates/core/src/multipart.rs:2291`; `crates/custodian/src/restore.rs:834`). |
| T2 Shape | PASS | Seven affected files contain focused helpers, tests and current operator guidance; no dependencies, clock reads, unsafe code or capability-probe workaround were introduced (`patch.diff:1`; `crates/custodian/src/restore.rs:878`; `crates/server/src/cli.rs:1408`). |
| T3 Runtime | FAIL | A damaged records obligation can make the second pass report clean and require no human despite the original bad segment remaining; the CLI uses that predicate for its verdict (same defect as C5; `crates/custodian/src/restore.rs:946`, `crates/server/src/cli.rs:1440`; `reviewer-evidence/identity.log:26`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design; the substantive contribution audit is owed at publish, as the deferred row explicitly records (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Confirm merged-history and closed/rejected-work coverage by affected path on the publication base: the brief documents restore.rs/multipart.rs prior art, but this target exposes only one synthetic commit and no remote, so broader coverage cannot be independently settled (`brief.md:145`; `reviewer-evidence/target-grounding.txt:2`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the slice's fitness for the writers-stopped restore procedure after the defect is resolved: live FoundationDB and the client/restore topology were not exercised, so behavioral evidence rests on raw-JSON in-memory doubles, not a recovery drill (`brief.md:132`; `crates/custodian/tests/restore_completing_fence.rs:35`; `docs/design/architecture/06-runtime-view.md:65`). |

One implementation finding warrants a rebuild: **[P2] bind the rerun's obligation to the session's retained segment nonce** (`crates/custodian/src/restore.rs:943`). The first fence correctly derives the group from the session. On a later pass, `Plan::Fenced` retains only the upload ID and epoch (`crates/custodian/src/restore.rs:900`), and `recheck_fenced` trusts the obligation's group. Key-taking decode validates its epoch but explicitly cannot validate its nonce against a session (`crates/core/src/multipart.rs:3465`). An empty foreign range therefore produces success without inspecting the actual attempt.

The independent reproduction calls the patched production function without editing target source. It fences a session containing a segment whose chunk belongs to no part; the first report correctly names it. It then replaces only the records obligation with a decodable same-epoch foreign nonce and reruns. The bad segment remains, but the second report has `segments_unaccounted: []`, `is_clean=true`, and `needs_human=false` (`reviewer-evidence/identity.log:26`). This also models an already-Aborting session in a damaged restored image. A separate positive control confirms the first fence handles a session nonce different from its upload ID (`reviewer-evidence/identity.log:25`).

Preserve the session nonce through the rerun plan, validate the obligation against that identity, and report a mismatch instead of issuing an all-clear. Add a regression covering the decodable foreign obligation. This is the same finding recorded by the frozen batch review (`gate-logs/T4-batch-review.log:10`), independently reproduced, not a second defect. It does not concern the settled half-drained-range deferral to #659 (`crates/custodian/src/restore.rs:921`).

The reproduction is retained in `pdca-reviewer-842-probe/tests/identity.rs:698`. From this review directory, run:

```sh
CARGO_NET_OFFLINE=true CARGO_TARGET_DIR="$PWD/pdca-reviewer-842-build" \
  cargo test --manifest-path pdca-reviewer-842-probe/Cargo.toml \
  --test identity reviewer_ -- --nocapture
```

Verification supports the intended fence, with these limits:

- **Independent execution:** seven tests failed before the fix and passed afterward; all nine Open-fence tests also passed. The target patch was restored byte-for-byte (`reviewer-evidence/rerun-status.txt:1`). Local CI completed spelling, docs lint/render, repository guards, formatting, Clippy, build and workspace tests, including the added core teardown test (`reviewer-evidence/ci.log:2`, `reviewer-evidence/ci.log:882`). Separate dependency and shared-global-state scans passed (`reviewer-evidence/scanners.log:3`, `reviewer-evidence/scanners.log:9`).
- **Host caveat:** local CI stopped at cargo-deny's read-only advisory-database lock (`reviewer-evidence/ci.log:3354`); it did not independently complete the later CI stages. The full frozen CI output records their successful completion (`gate-logs/C4-ci.log:3298`, `gate-logs/C4-ci.log:3919`). The separately attempted `deploy-guard` name is not a public xtask subcommand (`reviewer-evidence/scanners.log:12`); its frozen internal CI execution is recorded at `gate-logs/C4-ci.log:3331`. These are rerun limitations, not patch defects. Both external tools named in the brief, typos and docs-renderer, actually ran successfully.
- **Frozen evidence:** the mutation wrapper reports 7 caught and 26 unviable mutants, with none missed; that is not proof that the unviable cases were exercised (`gate-logs/C5-mutants.log:13`). The TiKV/server feature compilation log completes successfully (`gate-logs/host-tikv.log:209`). Instance-scoped wrappers were adjudicated from their supplied logs; none is missing.
- **Base/coverage caveat:** diff coverage was not measured because its wrapper could not apply the patch on origin/main (`gate-logs/C4-diff-cov.log:10`). The brief requires main plus prerequisite children (`brief.md:5`), and the supplied target passes the patch's reverse-apply check and compiles. This is not evidence of a patch compile defect (`reviewer-evidence/target-grounding.txt:9`).

The previous iteration's paging and undecodable-obligation test gaps are addressed: the sparse test references the final part, H/K include a bad segment beyond the first page, K includes an unreadable records obligation, and core now tests `completing_teardown` (`crates/custodian/tests/restore_completing_fence.rs:497`, `crates/custodian/tests/restore_completing_fence.rs:548`, `crates/custodian/tests/restore_completing_fence.rs:611`; `crates/core/src/multipart.rs:5212`). Seeded DST remains explicitly deferred to #843 and drain interaction to #659; neither is reopened here. No INTEGRATION.md was supplied or found in the target, so no additional project-specific human-only list could be independently applied.

### Advisory — adversary

# Adversarial review — #842 (restore fence for `Completing` sessions)

I re-ran the green leg in a scratch copy of `$PDCA_TARGET`: all 7 tests in
`restore_completing_fence.rs` pass. The frozen red leg (`gate-logs/C4-verify.log`) is red by
assertion on every test, not by a compile error. Every leg calls the production
`reconcile_after_restore`. The three gaps the last round found are now covered: the `seg:` page
boundary (case `paged`), the `part:` page boundary (G-sparse's segment on part 19,999), and the
undecodable records obligation (K's `torn` session). I then wrote extra cases and mutants against
the patched source. One production defect holds up (it is the same one T4's gating batch review
blocks on). Two more are test gaps that let a plausible regression through.

- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:943-948`: `recheck_fenced` trusts whatever
  group the `retire:records:s:<id>:<E'-1>` obligation names. It never checks that group against the
  session's own `(segment_nonce, E'-1)`. When the payload owes no segments, it skips silently
  (`None => Ok(())`). I ran two failing cases (scratch test, production code unchanged):
  **(A1)** an `Aborting@4` session with nonce `a7…`, `seg:a7…:3:000000` = `not a segment`, and a
  decodable `retire:records:s:a7…:3` = `{"seg":{"nonce":"9f…","epoch":3}}`. The pass reads the
  empty `seg:9f…:3:` range and returns `segments_unaccounted: []` with `needs_human() == false`.
  The session's own segment records have no deleter (X57 is open again) and nothing names them.
  **(A2)** the same setup, but the obligation is `{"parts":[[1,1]]}`, which decodes. The result is
  the same: clean, unnamed. This is the exact obligation shape leg G-collision seeds
  (`tests/restore_completing_fence.rs:455`). A session named "key taken" and then torn down by
  hand, as the CLI tells the operator to do, reaches A1 on the next run. The brief's "fact Do can
  rely on" (K) says *who* files that key. It does not say the payload names the session's own
  group, so trusting the payload is unwarranted. Fix shape: carry `record.segment_nonce()` in
  `Plan::Fenced` (`restore.rs:900-903`). Compare it with `payload.segments()`. On a mismatch or
  `None`, name the session (a new `SegmentFault` variant) and check the session's own range. Add
  A1 to leg K. The `deferred: #659` marker covers half-drained ranges, not a foreign group, so
  that deferral does not settle this.

- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:996-1000` (`segment_fault`): no test
  segment carries more than one chunk. `seed_segment` always builds `vec![chunk]`
  (`tests/restore_completing_fence.rs:254`). The mutant `.chunks().iter().take(1).find(..)` (check
  only the first chunk) **survives all 7 tests**. Real segments hold many chunks. Failing case for
  the mutant: one segment naming `[held, 0xD2F]`. The mutant reports the session clean. Production
  (`.find` over all chunks) names it. Fix: make H(ii)'s bad chunk the second chunk of a two-chunk
  segment.

- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:835-837`: G-collision seeds only the
  **records** key. Nothing tests a `Completing` session whose **bytes** key is already taken. The
  only bytes-key collision test is child-3's, and it covers `Open` sessions, which have one key. The
  mutant `for obligation in keys.iter().rev().take(1)` (require-absent on the last key only)
  **survives all 16 fence tests** (7 here plus 9 in `restore_open_fence.rs`). Failing case: a
  `Completing@3` session beside `retire:bytes:s:<id>:3` = `{"session":true}`. The mutant overwrites
  that obligation and fences the session. Production keeps it byte-identical and names
  `ObligationKeyTaken { key: "retire:bytes:s:…:3" }` (I checked both). The leg's own title is
  "Neither obligation overwrites one already there". Fix: add this as a second arm of G-collision.

- Not a refutation: C4-diff-cov's "patch.diff does not apply on origin/main" happens because this
  bundle's base is `origin/main` plus the unmerged child-1 to child-3 patches. The gate could not
  measure coverage. It is not a defect in the fix. The two mutants above partly stand in for that
  missing measurement.

- Attempted to refute these and could not:
  - `Completing@0`: fenced to `Aborting@1`, and the second pass is clean.
  - `Completing@u64::MAX`: no write and named (H(vi), plus the updated `restore_open_fence.rs`
    case).
  - A `seg:…:999999` key: this equals `MAX_SEGMENT_INDEX`, so it parses as one of the group's keys,
    and its value names a held chunk, so it is harmless.
  - Commit atomicity: a failure on any one of the three keys lands none of them.
  - The value ceiling under a real 10,000-part sparse set.
  - The fence still runs after Pass 3.
  - `needs_human` and `is_clean` include `segments_unaccounted`.
  - The blueprint's "five different bills" count matches the five bills it lists.

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:946`: The repeat-pass check trusts the retirement obligation's segment nonce without comparing it with the session's retained `segment_nonce`. For an `Aborting@4` session with nonce A, a decodable `retire:records:s:<id>:3` pointing to an empty group B makes this check succeed even when `seg:A:3:*` contains unaccounted chunks. The decoder checks the epoch only (`crates/core/src/multipart.rs:3506`); with otherwise clean metadata, the run reports `needs_human() == false`. Carry the expected nonce through `Plan::Fenced`, report a mismatched obligation, and add a regression with a foreign empty group and a faulty actual attempt range. This confirms the finding in the frozen T4 review evidence.

No additional correctness or material reuse, simplification, or efficiency findings. Validation used the target source and frozen gate logs; no builds were rerun or target files modified.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Rebuild the rerun identity check and add its regression: a decodable foreign-group obligation suppresses an existing warning about the real attempt, defeating the “still names what needs a human” guarantee (`crates/custodian/src/restore.rs:946`; `reviewer-evidence/identity.log:28`).
- [ ] T5 Judgment — Confirm merged-history and closed/rejected-work coverage by affected path on the publication base: the brief documents restore.rs/multipart.rs prior art, but this target exposes only one synthetic commit and no remote, so broader coverage cannot be independently settled (`brief.md:145`; `reviewer-evidence/target-grounding.txt:2`).
- [ ] Validation — fitness-to-purpose — Accept the slice's fitness for the writers-stopped restore procedure after the defect is resolved: live FoundationDB and the client/restore topology were not exercised, so behavioral evidence rests on raw-JSON in-memory doubles, not a recovery drill (`brief.md:132`; `crates/custodian/tests/restore_completing_fence.rs:35`; `docs/design/architecture/06-runtime-view.md:65`).
- [ ] `crates/custodian/src/restore.rs:943-948`: `recheck_fenced` trusts whatever group the `retire:records:s:<id>:<E'-1>` obligation names. It never checks that group against the session's own `(segment_nonce, E'-1)`. When the payload owes no segments, it skips silently (`None => Ok(())`). I ran two failing cases (scratch test, production code unchanged): **(A1)** an `Aborting@4` session with nonce `a7…`, `seg:a7…:3:000000` = `not a segment`, and a decodable `retire:records:s:a7…:3` = `{"seg":{"nonce":"9f…","epoch":3}}`. The pass reads the empty `seg:9f…:3:` range and returns `segments_unaccounted: []` with `needs_human() == false`. The session's own segment records have no deleter (X57 is open again) and nothing names them. **(A2)** the same setup, but the obligation is `{"parts":[[1,1]]}`, which decodes. The result is the same: clean, unnamed. This is the exact obligation shape leg G-collision seeds (`tests/restore_completing_fence.rs:455`). A session named "key taken" and then torn down by hand, as the CLI tells the operator to do, reaches A1 on the next run. The brief's "fact Do can rely on" (K) says *who* files that key. It does not say the payload names the session's own group, so trusting the payload is unwarranted. Fix shape: carry `record.segment_nonce()` in `Plan::Fenced` (`restore.rs:900-903`). Compare it with `payload.segments()`. On a mismatch or `None`, name the session (a new `SegmentFault` variant) and check the session's own range. Add A1 to leg K. The `deferred: #659` marker covers half-drained ranges, not a foreign group, so that deferral does not settle this.
- [ ] `crates/custodian/src/restore.rs:996-1000` (`segment_fault`): no test segment carries more than one chunk. `seed_segment` always builds `vec![chunk]` (`tests/restore_completing_fence.rs:254`). The mutant `.chunks().iter().take(1).find(..)` (check only the first chunk) **survives all 7 tests**. Real segments hold many chunks. Failing case for the mutant: one segment naming `[held, 0xD2F]`. The mutant reports the session clean. Production (`.find` over all chunks) names it. Fix: make H(ii)'s bad chunk the second chunk of a two-chunk segment.
- [ ] `crates/custodian/src/restore.rs:835-837`: G-collision seeds only the **records** key. Nothing tests a `Completing` session whose **bytes** key is already taken. The only bytes-key collision test is child-3's, and it covers `Open` sessions, which have one key. The mutant `for obligation in keys.iter().rev().take(1)` (require-absent on the last key only) **survives all 16 fence tests** (7 here plus 9 in `restore_open_fence.rs`). Failing case: a `Completing@3` session beside `retire:bytes:s:<id>:3` = `{"session":true}`. The mutant overwrites that obligation and fences the session. Production keeps it byte-identical and names `ObligationKeyTaken { key: "retire:bytes:s:…:3" }` (I checked both). The leg's own title is "Neither obligation overwrites one already there". Fix: add this as a second arm of G-collision.
- [ ] `crates/custodian/src/restore.rs:946`: The repeat-pass check trusts the retirement obligation's segment nonce without comparing it with the session's retained `segment_nonce`. For an `Aborting@4` session with nonce A, a decodable `retire:records:s:<id>:3` pointing to an empty group B makes this check succeed even when `seg:A:3:*` contains unaccounted chunks. The decoder checks the epoch only (`crates/core/src/multipart.rs:3506`); with otherwise clean metadata, the run reports `needs_human() == false`. Carry the expected nonce through `Plan::Fenced`, report a mismatched obligation, and add a regression with a foreign empty group and a faulty actual attempt range. This confirms the finding in the frozen T4 review evidence.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_842/review-b
- [ ] **Atomicity alone does not protect the new retirement obligation from overwrite.** G requires three writes in one commit and a failing-commit witness (`brief.md:35-43`), but never requires a collision witness for the newly added `retire:records:s:<id>:<E>` key. The target explicitly requires `require_absent` on obligation installation and classification of collisions, because overwriting a payload permanently loses reclamation evidence (`crates/core/src/multipart.rs:1409-1414`; `docs/design/proposals/draft/0016-multipart-commit-protocol.md:369-373`). A blind put of the records obligation can satisfy the listed fresh-store and atomicity cases. Revise G to require the absence guard and seed an existing, decodable same-epoch obligation naming a different segment group: its bytes must survive, none of the three fence writes may land, and the unresolved collision must be reported.
- [ ] **The promised epoch transition has an accepted input with no successor.** G says `Completing@E` ends at `Aborting@E+1` (`brief.md:35-38`), and the invariant is unconditional (`brief.md:76-79`). On the supplied target, the session epoch is any `u64` (`crates/core/src/multipart.rs:2085`); Completing validation checks cursor bounds and matching target identity/epoch, but does not reject `u64::MAX` (`crates/core/src/multipart.rs:2204-2240`). Such a decodable record cannot take the required transition. Revise H to cover a Completing record at that boundary: checked increment, no wrap/panic or partial obligations, unchanged session reported as needing a human, and continued fencing of a later eligible session. Explicitly apply any inherited child-3 guard to Completing, and qualify the invariant for sessions left unresolved instead of promising universal fencing.
- [ ] T5 Judgment — Confirm the path-based prior-art disposition against merged history and closed/rejected work — the brief records that check, but this one-commit snapshot has no remote or historical review evidence to corroborate it (`brief.md:145`, `pdca-reviewer-842-evidence/source-audit.log:10`).

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
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild the rerun identity check and add its regression: a decodable foreign-group obligation suppresses an existing warning about the real attempt, defeating the “still names what needs a human” guarantee (`crates/custodian/src/restore.rs:946`; `reviewer-evidence/identity.log:28`).; `crates/custodian/src/restore.rs:943-948`: `recheck_fenced` trusts whatever group the `retire:records:s:<id>:<E'-1>` obligation names. It never checks that group against the session's own `(segment_nonce, E'-1)`. When the payload owes no segments, it skips silently (`None => Ok(())`). I ran two failing cases (scratch test, production code unchanged): **(A1)** an `Aborting@4` session with nonce `a7…`, `seg:a7…:3:000000` = `not a segment`, and a decodable `retire:records:s:a7…:3` = `{"seg":{"nonce":"9f…","epoch":3}}`. The pass reads the empty `seg:9f…:3:` range and returns `segments_unaccounted: []` with `needs_human() == false`. The session's own segment records have no deleter (X57 is open again) and nothing names them. **(A2)** the same setup, but the obligation is `{"parts":[[1,1]]}`, which decodes. The result is the same: clean, unnamed. This is the exact obligation shape leg G-collision seeds (`tests/restore_completing_fence.rs:455`). A session named "key taken" and then torn down by hand, as the CLI tells the operator to do, reaches A1 on the next run. The brief's "fact Do can rely on" (K) says *who* files that key. It does not say the payload names the session's own group, so trusting the payload is unwarranted. Fix shape: carry `record.segment_nonce()` in `Plan::Fenced` (`restore.rs:900-903`). Compare it with `payload.segments()`. On a mismatch or `None`, name the session (a new `SegmentFault` variant) and check the session's own range. Add A1 to leg K. The `deferred: #659` marker covers half-drained ranges, not a foreign group, so that deferral does not settle this.; `crates/custodian/src/restore.rs:996-1000` (`segment_fault`): no test segment carries more than one chunk. `seed_segment` always builds `vec![chunk]` (`tests/restore_completing_fence.rs:254`). The mutant `.chunks().iter().take(1).find(..)` (check only the first chunk) **survives all 7 tests**. Real segments hold many chunks. Failing case for the mutant: one segment naming `[held, 0xD2F]`. The mutant reports the session clean. Production (`.find` over all chunks) names it. Fix: make H(ii)'s bad chunk the second chunk of a two-chunk segment.; `crates/custodian/src/restore.rs:835-837`: G-collision seeds only the **records** key. Nothing tests a `Completing` session whose **bytes** key is already taken. The only bytes-key collision test is child-3's, and it covers `Open` sessions, which have one key. The mutant `for obligation in keys.iter().rev().take(1)` (require-absent on the last key only) **survives all 16 fence tests** (7 here plus 9 in `restore_open_fence.rs`). Failing case: a `Completing@3` session beside `retire:bytes:s:<id>:3` = `{"session":true}`. The mutant overwrites that obligation and fences the session. Production keeps it byte-identical and names `ObligationKeyTaken { key: "retire:bytes:s:…:3" }` (I checked both). The leg's own title is "Neither obligation overwrites one already there". Fix: add this as a second arm of G-collision.; `crates/custodian/src/restore.rs:946`: The repeat-pass check trusts the retirement obligation's segment nonce without comparing it with the session's retained `segment_nonce`. For an `Aborting@4` session with nonce A, a decodable `retire:records:s:<id>:3` pointing to an empty group B makes this check succeed even when `seg:A:3:*` contains unaccounted chunks. The decoder checks the epoch only (`crates/core/src/multipart.rs:3506`); with otherwise clean metadata, the run reports `needs_human() == false`. Carry the expected nonce through `Plan::Fenced`, report a mismatched obligation, and add a regression with a foreign empty group and a faulty actual attempt range. This confirms the finding in the frozen T4 review evidence.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_842/review-b. 4 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
