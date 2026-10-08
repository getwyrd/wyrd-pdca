# custodian: restore fences resurrected Completing sessions with their segments' deleter (809.4)

> Child 4 of 5 of #809's split at its re-plan (2026-09-29); #809 is itself 664.2. Do reads ONLY
> this file. Keep the `- **Label:** value` lines. `path:line` citations are on `origin/main` @
> `243241e` (verified 2026-09-29). This bundle's base is `origin/main` **plus child-1's,
> child-2's and child-3's accepted patches**: locate the `Open` fence, `sessions_fenced`,
> `sessions_unsettled` and the `segment_nonce` accessor by symbol. Background: 0016 =
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`; the `Completing → Aborting`
> fence row at `0016:665`; X57's test row at `0016:880`.

- **Slug:** restore-fence-completing
- **Kind:** enhancement
- **Defect:** after child-3, restore names a resurrected `Completing` session but does not
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
- **Success criterion:** the NEW file `crates/custodian/tests/restore_completing_fence.rs`
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
- **Falsifiability:** RED on its base (`origin/main` + child-1..3), in-process, by assertion.
  Child-3's pass names a decodable `Completing` session and leaves it untouched, so G, G-sparse,
  H(ii), H(iv), H(v) and K all fail there (the session is not fenced). H(i) and H(vi) already
  pass on the base, since child-3 names an undecodable record and leaves every decodable
  `Completing` one untouched and named; they stay as regression legs. G-collision fails on the
  base only through its "key taken" cause, so it earns its keep against a blind-put mutant: pair
  it in the same test with G's positive arm on a second, uncollided session. The test
  names only symbols on its base and none this child adds. A compile failure on the red leg
  reports UNVERIFIABLE (`engine/scripts/run-verify.sh:533-541`). Record in `build-notes.md` how
  many ran red.
- **Invariant to restore:** after the pass, every session the restored image held `Open` or
  `Completing` is either fenced (so it cannot publish) or named as needing a human; none is left
  unfenced and unnamed. Every record a fenced session wrote (its parts, its staged residue, its
  segment records) has a named deleter installed in the same commit as its fence, and no fence
  overwrites an obligation already in the store. No value the fence writes can exceed the store's
  value ceiling. Source: 0016 D-B and 1.4 (`:717-728`), `:665`, X57 (`:880`), `:2193`,
  `:369-373`; `metadata.rs:546-549` (the ceiling every backend inherits); ADR-0045. SELF-TEST:
  fencing `Completing` without the `seg:` obligation leaves X57 open, fencing it with
  `{session, parts}` fails G-sparse, and a blind put of the records obligation fails
  G-collision.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 840, 841
- **Ordering note:** wave 3 of #809's split. child-3 is a prerequisite because this child extends
  its fence, report fields and doc paragraph; child-2 because this child reads its nonce.
  child-5 builds on this child. **#810 must depend on this child**, not on #809, which the split
  closes. Downstream, not work here: 0016's reaper and operator-abort `Completing → Aborting`
  rows (`:665`, `:2193`) still specify `{session, parts}` and hit the same ceiling. Flag that to
  #656 and #659, which own those doors; do not edit 0016 here.
- **Surfaces:** data
- **Difficulty:** high
- **Do model:** opus
- **Scope:** the `Completing` arm of the restore fence: one commit per session installing the
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
- **Repro instruction:** on its base, seed a `Completing@3` session with its nonce, two `part:`
  records and `seg:<nonce>:3:0`, and run `reconcile_after_restore`. The session is still
  `Completing@3`, no `retire:` key exists, and the session is named as needing a human.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/restore_completing_fence.rs`. This is a **NEW** file:
  C4-verify's red comes only from an added `*/tests/*.rs` (`run-verify.sh:141-144`). Keep the
  doubles inside it, including the value-ceiling double.
- **Production reach:** the pass under test is the production `reconcile_after_restore`. Every
  session is seeded by the test, because no client can create one until #508. A live
  FoundationDB would refuse an oversized value; here the double enforces the same ceiling.
- **Citations expected:** Do must cite `path:line` on the target branch for every change. Peer
  callsites Do MAY open and mirror:
  * Child-3's fence and its test doubles, by symbol on the base.
  * `crates/core/src/multipart.rs:2021-2030` (the `Completing` variant) and child-2's group
    accessor; `:3319-3355` (`checked_against_key`: a `seg` group's epoch must equal its token's
    epoch); `:3296-3303` (the `all` wildcard's rule).
  * `crates/core/src/metadata.rs:1343` / `:1462` (`SegmentRecord` and its decode), `:1480`
    (`seg_key`), `:1505` (`seg_range_prefix`), `:1527` (`parse_seg_key`), `:546-603`
    (`MAX_VALUE_BYTES`, `flat_value_ceiling_crossed`).
  * `crates/custodian/tests/staged_protection.rs:154-400` (the hooked store double).
- **Prior-art check (triage cycles):** by path (`restore.rs`, `multipart.rs`), 2026-09-29 on
  `243241e`. No merged change fences sessions, and no open PR touches these paths. Rejected prior
  art: #809 iteration 1 used `{session, parts}` (overflow, leg G-sparse) and left the
  unreadable/stray `seg:` branches untested (legs H(iv), H(v)). #637 iteration 1 fenced
  `Completing` sessions without the `seg:` deleter. #664 iteration 1 had the second-pass bug
  (leg K). Do not repeat any of them unchanged.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

Plan-review response (2026-09-29): both findings revised in place. (1) New leg G-collision:
both obligation keys go in under `require_absent` (`0016:369-373`, `:675-676`); a different,
decodable `retire:records:s:<id>:<E>` survives byte-identical, none of the three writes lands,
the session is named with child-3's "key taken" cause, and later sessions are still fenced.
(2) New H(vi): a decodable `Completing@u64::MAX` gets child-3's guard (no transition, no
obligation, no wrap or panic, named, later sessions fenced). The invariant now says every such
session is fenced **or** named, not that every one is fenced. Fixtures now follow #840's
placement A (the human, 2026-09-29): `segment_nonce` on every session record, and the attempt's
group is `(nonce, publish_target.epoch)`.

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — `crates/custodian/src/restore.rs:985`: the seg-range paging in `check_attempt` is never exercised. Mutant `match (next.filter(|_| false), page.last())` (stop after the first page) **survives all 16 fence tests**. Failing case: a `Completing@3` session with 513 `seg:` records (`STAGED_PAGE` = 512, `crates/custodian/src/gc.rs:307`) whose bad record (undecodable, or naming an unheld chunk) is index 512. The mutant fences it, does not name it, and exits 0. The production code pages correctly today, but nothing pins it. Add that case to leg H or K.; `crates/custodian/src/restore.rs:1028`: the part-range paging in `part_chunks` is never exercised either. Mutant "first page of `part:` only" **survives**. G-sparse already seeds 10,000 parts, but its two segments name only the chunks of parts 1 and 3 (`crates/custodian/tests/restore_completing_fence.rs:510-511`), and both sit on page one. Failing case for the mutant: point a segment at the chunk of part 19,999. The clean sparse session would then be wrongly named `ChunkInNoPart`, and G-sparse's `!names(...)` would go red. This is a one-line test change.; `crates/custodian/src/restore.rs:952-958`: the `recheck_fenced` arm that names an **undecodable `retire:records:s:<id>:<E-1>` obligation** has no test. Replacing it with `Err(_) => Ok(())` **survives**. Failing case for the mutant: an `Aborting@4` session beside `retire:records:s:<id>:3` = `not json`. Production names it (`SegmentFault::Undecodable`); the mutant reports that run as needing no human, and the run still exits 0 (fencing work alone is not a failure, per `cli.rs` `restore_verdict`). Also, in this arm the named "record" is the obligation key. The CLI paragraph (`crates/server/src/cli.rs` NEEDS-HUMAN text, diff hunk @@ -1397) calls every named record a *segment record* ("wrote segment records that include one nothing accounts for"), so an operator would go looking under `seg:` for a `retire:` key.; `crates/core/src/multipart.rs:2294-2306`: `completing_teardown` has no unit test in `wyrd-core`, while `open_teardown` has one (`multipart.rs:5147-5197`). That is why C5 reports `multipart.rs:2300:17` (delete `state` field) as MISSED: cargo-mutants runs only wyrd-core's own suite for a core file. I confirmed the custodian test kills that mutant (5 of 7 fail), so this is **not** a behaviour gap. A `mod completing_teardown` twin of `mod open_teardown` would make the C5 gate's row reflect that. It should cover the session at `Aborting@E+1`, both keys decoding against their payloads, and `None` for Open/Aborting/Completed/`u64::MAX`.. 3 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 39 mutants tested in 4m: 2 missed, 8 caught, 29 unviable
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild the rerun identity check and add its regression: a decodable foreign-group obligation suppresses an existing warning about the real attempt, defeating the “still names what needs a human” guarantee (`crates/custodian/src/restore.rs:946`; `reviewer-evidence/identity.log:28`).; `crates/custodian/src/restore.rs:943-948`: `recheck_fenced` trusts whatever group the `retire:records:s:<id>:<E'-1>` obligation names. It never checks that group against the session's own `(segment_nonce, E'-1)`. When the payload owes no segments, it skips silently (`None => Ok(())`). I ran two failing cases (scratch test, production code unchanged): **(A1)** an `Aborting@4` session with nonce `a7…`, `seg:a7…:3:000000` = `not a segment`, and a decodable `retire:records:s:a7…:3` = `{"seg":{"nonce":"9f…","epoch":3}}`. The pass reads the empty `seg:9f…:3:` range and returns `segments_unaccounted: []` with `needs_human() == false`. The session's own segment records have no deleter (X57 is open again) and nothing names them. **(A2)** the same setup, but the obligation is `{"parts":[[1,1]]}`, which decodes. The result is the same: clean, unnamed. This is the exact obligation shape leg G-collision seeds (`tests/restore_completing_fence.rs:455`). A session named "key taken" and then torn down by hand, as the CLI tells the operator to do, reaches A1 on the next run. The brief's "fact Do can rely on" (K) says *who* files that key. It does not say the payload names the session's own group, so trusting the payload is unwarranted. Fix shape: carry `record.segment_nonce()` in `Plan::Fenced` (`restore.rs:900-903`). Compare it with `payload.segments()`. On a mismatch or `None`, name the session (a new `SegmentFault` variant) and check the session's own range. Add A1 to leg K. The `deferred: #659` marker covers half-drained ranges, not a foreign group, so that deferral does not settle this.; `crates/custodian/src/restore.rs:996-1000` (`segment_fault`): no test segment carries more than one chunk. `seed_segment` always builds `vec![chunk]` (`tests/restore_completing_fence.rs:254`). The mutant `.chunks().iter().take(1).find(..)` (check only the first chunk) **survives all 7 tests**. Real segments hold many chunks. Failing case for the mutant: one segment naming `[held, 0xD2F]`. The mutant reports the session clean. Production (`.find` over all chunks) names it. Fix: make H(ii)'s bad chunk the second chunk of a two-chunk segment.; `crates/custodian/src/restore.rs:835-837`: G-collision seeds only the **records** key. Nothing tests a `Completing` session whose **bytes** key is already taken. The only bytes-key collision test is child-3's, and it covers `Open` sessions, which have one key. The mutant `for obligation in keys.iter().rev().take(1)` (require-absent on the last key only) **survives all 16 fence tests** (7 here plus 9 in `restore_open_fence.rs`). Failing case: a `Completing@3` session beside `retire:bytes:s:<id>:3` = `{"session":true}`. The mutant overwrites that obligation and fences the session. Production keeps it byte-identical and names `ObligationKeyTaken { key: "retire:bytes:s:…:3" }` (I checked both). The leg's own title is "Neither obligation overwrites one already there". Fix: add this as a second arm of G-collision.; `crates/custodian/src/restore.rs:946`: The repeat-pass check trusts the retirement obligation's segment nonce without comparing it with the session's retained `segment_nonce`. For an `Aborting@4` session with nonce A, a decodable `retire:records:s:<id>:3` pointing to an empty group B makes this check succeed even when `seg:A:3:*` contains unaccounted chunks. The decoder checks the epoch only (`crates/core/src/multipart.rs:3506`); with otherwise clean metadata, the run reports `needs_human() == false`. Carry the expected nonce through `Plan::Fenced`, report a mismatched obligation, and add a regression with a foreign empty group and a faulty actual attempt range. This confirms the finding in the frozen T4 review evidence.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_842/review-b. 4 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_842/review-b
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): rebuilding for the implementation-level findings — C5 Causal adequacy — Repair the missing-deleter rerun case and pin it with a regression — surviving attempt records can lose an existing warning and receive a clean restore verdict (`crates/custodian/src/restore.rs:945`, `reviewer-evidence/missing-obligation.log:11`).; `crates/custodian/src/restore.rs:945-947`: `recheck_fenced` returns `Ok(())` as soon as `retire:records:s:<id>:<E'-1>` is **absent**. It never looks at the session's own `seg:<nonce>:<E'-1>:` range. That is the T4 gate's blocking finding (all 3 T4 bullets are this one line), and I confirmed it with two failing cases in a scratch test. The production code is unchanged in both. **(A1)** `Aborting@4` session `aa…`, `seg:aa…:3:000000` = a valid segment, `seg:aa…:3:000001` = `not a segment`, and no `retire:records:s:aa…:3`. The result is `segments_unaccounted: []`, `needs_human() == false`, and the CLI exits 0. Those segment records have no deleter anywhere (X57 is open again), and nothing names them. **(A2)** This is the operator flow the patch itself invites. Take an `Aborting@4` session with `retire:records:s:<id>:3` = `not json` (leg K's `fb` shape, `tests/restore_completing_fence.rs:610`) and one live `seg:<nonce>:3:000000`. Pass 1 correctly names the obligation `Undecodable`. The CLI then says "inspect the named record … then re-run this pass" (`crates/server/src/cli.rs:1411`). The obvious repair for an undecodable obligation is to delete it. When the operator does that and re-runs, pass 2 reports **clean** (`needs_human() == false`). The same happens with K's `fc` (`{parts}` only, `NotOfAttempt`). This breaks the brief's K rule ("Already `Aborting` never means nothing to report"). The `deferred: #659` marker (`restore.rs:929-930`) covers a *half-drained* range read as false positives. It does not cover this false negative, so the deferral does not settle it. **The fix is cheap and has no false alarms.** On an absent key, read one `staged_page` of `seg_range_prefix(group)`. If it is non-empty, name the session at its first record with a new `SegmentFault` (e.g. "no records obligation owes it"). My control case (`Open`-fenced `Aborting@4`, empty range at 3) stays unnamed. A finished #659 drain empties the range before it drops the key, so that case stays unnamed too. Add A1 and A2 to leg K, and update the `06-runtime-view.md:65` sentence that lists what "every run also names".; `crates/custodian/src/restore.rs:925-928`: the doc comment reasons from the brief's fact ("only that fence files `retire:records:s:<id>:<E'-1>`") to its converse (no key, so no attempt, so nothing to check). The brief never claims the converse, and A1 shows it is false for a damaged or hand-repaired store. Reword the comment along with the fix above. The reviewer's acceptance of leg K rests on this converse.; `crates/custodian/src/restore.rs:945`: A missing records obligation silently skips the attempt’s segment range. For an `Aborting@4` session with nonce A, no `retire:records:s:<id>:3`, and an undecodable `seg:A:3:000000`, this branch returns without naming either record. With otherwise empty metadata and fragment stores, the pass can report `is_clean() == true` and `needs_human() == false`, despite remaining segments having no deleter. Probe the attempt range before accepting an absent obligation; if records remain, report the missing deleter even when those records decode. Add this regression alongside the malformed/foreign obligation cases at `crates/custodian/tests/restore_completing_fence.rs:608`, retaining a clean control with neither obligation nor segments.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_842/review-b. 4 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_842/review-b
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 4): rebuilding for the implementation-level findings — `crates/custodian/src/restore.rs:1491` (`emit_segments_unaccounted`) is pinned by no test. Running cargo-mutants on the diff with `--cap-lints=true`, the mutant `replace emit_segments_unaccounted with ()` **survives** the whole wyrd-custodian suite. A second mutant survives too: `restore.rs:266` (`Display for SegmentFault` → empty string). Why it matters: the CLI names only 20 sessions, then says "and N more (the audit log names every one)" (`crates/server/src/cli.rs:1402-1416`, `named_records` at `:1463-1474`). The blueprint sends operators to `action=session-segments-unaccounted` too. Failing case under the mutant: 21 fenced `Completing` sessions, each holding one undecodable `seg:` record. The 21st is then named nowhere: not on stdout, and not in the audit log. Under the second mutant, the "and why: …" text prints blank. Fix: the test file already captures the audit seam (`AUDIT` and `audited()`, `crates/custodian/tests/restore_completing_fence.rs:336-371`), but only for `dangling` and `summary`. In leg H or K, assert that each named case emits a `session-segments-unaccounted` event carrying its `session`, its `record` and a non-empty `fault`. (The subscriber is installed once per binary, by the Order test, so the new leg must install it too, idempotently.); every fixture sets the session's `segment_nonce` equal to its upload id (`restore_completing_fence.rs:182-196`; `group()` at `:219`). So no test can tell the two apart. Concrete survivor, which I ran: change `restore.rs:911` from `SegmentGroup::from_nonce(record.segment_nonce().clone(), attempt)` to `SegmentGroup::new(upload.as_str(), attempt).expect(..)`. All 7 tests in `restore_completing_fence.rs` and all 9 in `restore_open_fence.rs` still pass. Under that mutant, a fenced session whose nonce differs from its id (`77…` vs `a7…`) is named `NotOfAttempt` on **every** re-run, because its own correct `{seg:(77…,3)}` obligation no longer matches. Its real `seg:77…:3:` range is never read, so a junk `seg:77…:3:000001` goes unnamed. That is a false alarm on every clean fence, and it hides the real fault (leg K). The production code is correct today: my probe with a distinct nonce passes on the patch as built. Fix: seed K's clean control and at least one damaged H/K case with a nonce that is not the upload id.. 8 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 6 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 5): rebuilding for the implementation-level findings — `crates/custodian/src/restore.rs:957`: the re-run check accepts a records obligation that owes the session's own segment group **plus** a part set. It compares only `owed.segments()`. Concrete case (my scratch probe, production code unchanged): an `Aborting@4` session, `retire:bytes:s:<id>:3` = `{"session":true,"parts":"all"}`, and `retire:records:s:<id>:3` = `{"parts":[[1,2]],"seg":{"nonce":"<own nonce>","epoch":3}}`, which decodes. With clean segments, the pass returns `segments_unaccounted: []` and `needs_human() == false`. The same test file names `{parts}` alone as damage (`fc`, `crates/custodian/tests/restore_completing_fence.rs:614`, `NotOfAttempt`). No writer files a `{parts}` records obligation for an aborted session: 0016:356 says only the publication batch does. Its drain would delete the `part:` records that `{session, all}` has to list at drain time, leaving part bytes unmarked. That is the X104 outcome (0016:2633). The doc comment just above the check (`restore.rs:928-930`) says "trusted only if it owes `group`: one owing anything else is the first record at fault", and the code does not do that. Fix: also require `owed.parts().is_none()` in the guard, and add this payload as a fifth `Aborting@4` case in leg K. Low severity: it needs a damaged store.; `crates/custodian/src/restore.rs:397` and `:421-427`: the rustdoc of the public `reconcile_after_restore` still says "every session the image holds `Open` is fenced" and describes only the `Open@E` / `{session, all}` commit. The patch added a paragraph at `:432-434` but left this heading and summary contradicting it. Related wording in `docs/design/architecture/06-runtime-view.md:65`: "no obligation already holds that key" is singular, but a `Completing` fence now requires two keys absent ("either key"). Doc-only nit.. 10 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Full previous attempt preserved in `iteration-v6/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
