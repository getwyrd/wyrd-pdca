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
