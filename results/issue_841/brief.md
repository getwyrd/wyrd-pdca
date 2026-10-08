# custodian: restore fences resurrected Open upload sessions (809.3)

> Child 3 of 5 of #809's split at its re-plan (2026-09-29); #809 is itself 664.2. Do reads ONLY
> this file. Keep the `- **Label:** value` lines. `path:line` citations are on `origin/main` @
> `243241e` (verified 2026-09-29). This bundle's base is `origin/main` **plus child-1's and
> child-2's accepted patches**: locate their additions (`staged_skipped`, `staged_untrusted`, the
> `segment_nonce` field and its accessor) by symbol. Background: 0016 =
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`; D-B and decision 1.4 at
> `0016:717-728`; the abort/restore fence row at `0016:664`; the restore row at `0016:823`; the
> D-B test row at `0016:879`.

- **Slug:** restore-fence-open
- **Kind:** enhancement
- **Defect:** the post-restore pass fences no upload session. A restored metadata image can
  resurrect a session that was torn down after the restore point, with its bytes already
  reclaimed, and nothing stops a retried Complete from publishing over them (D-B,
  `0016:717-728`; F13). `reconcile_after_restore` (`crates/custodian/src/restore.rs:312-589`)
  reads each session only by key, for staged protection (`crates/custodian/src/gc.rs:1505-1510`:
  "A session record's value is never decoded"), and never writes to it. 0016's restore fence
  moves every session open in the image to `Aborting@E+1`, in one batch with the retirement
  obligation that owes its records (`0016:664`, `:823`, `:717-728`), and the report counts it as
  `sessions_fenced` (`0016:823`, `:879`).
- **Success criterion:** the NEW file `crates/custodian/tests/restore_open_fence.rs` passes over
  in-memory doubles, with records seeded as raw JSON, calling the production
  `reconcile_after_restore`. Every seeded session record carries `segment_nonce` in child-2's
  spelling (immediately after `clock_source`, in every state): on this base a record without it
  does not decode, and would be taken for H(i). Legs:
  **(F) An `Open` session is fenced, whole.** An `Open@E` session with committed parts ends as
  `Aborting@E+1`. The same commit installs `retire:bytes:s:<id>:<E>` owing the session's staged
  residue **and every part**: `{session, all}`, the one payload shape that owes both
  (`crates/core/src/multipart.rs:3141-3142`, `0016:2187`). `{session}` alone would leave the
  committed parts with no deleter. The obligation decodes through `decode_retire_obligation`
  (`multipart.rs:3455`) against the key it sits under. The `Debug` rendering contains
  `sessions_fenced: 1`. A session already `Aborting` or `Completed` in the image is left
  byte-identical and not counted.
  **(F-atomic) The fence lands whole or not at all.** Use a store double that fails the commit
  carrying the session's write in one run, and the commit carrying the obligation's write in
  another. After either run, **neither** write is present and the pass returns `Err`. A commit
  whose outcome is unknown is never read as `Conflict` (`crates/traits/src/lib.rs:205-212`,
  `AGENTS.md:178-180`).
  **(F-race) A session that changes under the pass is not fenced blind.** A write lands on the
  session record between the pass's read of it and the fence's commit: a Complete fence moving it
  from `Open@E` to `Completing@E+1` (every fence bumps the epoch, `0016:704-708`,
  `multipart.rs:2173`). The fence then writes nothing: no `retire:` key names that session, and the
  concurrent write's bytes are intact. The session is named as needing a human, and the pass
  still fences every other session and returns `Ok`.
  **(F-collision) The fence never overwrites an obligation.** Installation is
  `require_absent(retire:<mode>:<token>)`, never a blind put (`0016:369-373`, and the note under
  the batch table, `0016:675-676`; `multipart.rs:1409-1414`). Seed an `Open@E` session beside a
  `retire:bytes:s:<id>:<E>` value that decodes but differs from what the fence would write (a
  `{session}` payload). After the pass, the session and that obligation are both byte-identical,
  nothing else was written for that session, the session is named as needing a human with a
  cause that says the obligation key was taken (not that the session changed, as in F-race),
  and the pass still fences every other session and returns `Ok`. A collision is classified
  once and named, never retried in a loop: the token grammar makes it impossible without damage
  (`0016:358-373`).
  **(H) What this pass cannot fence is named, never passed off as done.** Each of these is left
  byte-identical, has no `retire:` key written for it, and is named as needing a human
  (`needs_human()` true, `restore.rs:212`):
  (i) a session whose value will not decode (the staged class still protects its records exactly
  as on the base);
  (ii) an `Open` session at epoch `u64::MAX`, which has no `E+1`;
  (iii) a decodable `Completing` session. Its fence must also retire its segment records and is
  child-4's; until that lands, this pass must say it did not fence it.
  **(K) A second pass is idempotent.** Re-running over the fenced store leaves the whole store
  byte-identical to how the first pass left it (no second obligation, nothing fenced again), and
  every session H named is named again.
  **(P3) A fence fault never hides the pass's verdicts.** The store holds one dangling committed
  chunk, one under-replicated committed chunk, and one `Open` session whose fence commit fails.
  The pass returns `Err`, **and** before it the audit trail carries both verdicts: the chunk's
  `DANGLING` line (`emit_dangling`, `restore.rs:906`), and the pass's summary counts with
  `under_replicated` at 1. An under-replicated chunk has no line of its own; its only audit
  record is the summary (`emit_summary`, `restore.rs:1021-1032`), which the base emits only on
  the `Ok` path (`:587`). So on a fence fault the summary is still emitted, and it must not read
  "complete" (the fence did not finish). The CLI prints no verdict on `Err`
  (`crates/server/src/cli.rs:1187-1196`); that stays, and the audit trail is where the operator
  finds the counts. Do chooses how the error path emits them. The fence runs after Pass 3
  (`restore.rs:534-585`), never before it. This is the "no report AT ALL" class #651 fixed
  (`restore.rs:324-329`).
  **(Paging)** Sessions listed across more than one page of the `mpu:` listing are all fenced. A
  scan cap on the double forces the paging, as `staged_protection.rs:1067` does.
  **(CLI)** `restore_verdict` (`crates/server/src/cli.rs:1256`) counts fenced sessions in its
  summary, and prints a `NEEDS-HUMAN` paragraph for the sessions it could not fence, through
  `named_records` (`cli.rs:1389`) unchanged: the first 20 by name
  (`NAMED_UNREADABLE_RECORDS`, `cli.rs:1380`) and the rest as a count. `RestoreReport` and the
  audit trail name every one. Cover this in `cli.rs`'s report tests (green-only), including an
  over-limit case with 21 unsettled sessions, mirroring
  `restore_verdict_names_the_blocking_records_and_counts_the_ones_it_cannot_fit`
  (`cli.rs:2999-3023`); and extend the agreement test (from `cli.rs:2898`) to the new paragraph.
  Do not change `named_records`' bound.
  **(L) `cargo xtask ci` green.** This includes the existing restore legs in
  `staged_protection.rs` (among them the three writes-during-restore legs, `:1511-1540`) and the
  restore DST properties in `crates/dst/tests/custodian.rs:1954-2200`, unchanged.
- **Falsifiability:** RED on its base (`origin/main` + child-1 + child-2), in-process, by
  assertion: the base pass decodes no session and writes no fence, so every leg but CLI fails
  there (session still `Open@E`, no `retire:` key, no `sessions_fenced` in `Debug`, `Ok` where P3
  wants `Err`, `needs_human()` false for H). The test names only symbols on its base, none this
  child adds; new report fields are asserted through `Debug`. A red-leg compile failure reports
  UNVERIFIABLE (`engine/scripts/run-verify.sh:533-541`). Record in `build-notes.md` how many ran
  red.
- **Invariant to restore:** after the post-restore pass, no session the restored image held
  `Open` can publish, and every record that session wrote has a named deleter, installed in the
  same commit as its fence. A session the pass could not fence is named, and the pass's other
  verdicts survive any fault in the fence. Source: 0016 D-B and decision 1.4 (`:717-728`),
  `:664`, `:823`, `:879`; ADR-0045 (what cannot be decoded is left byte-identical and named);
  `AGENTS.md:178-180` (an unknown commit outcome is never a clean conflict); `0016:369-373` (an
  obligation is installed under `require_absent`, never over another). SELF-TEST: a fence placed
  before Pass 3 satisfies F and fails P3, a fence split across two commits satisfies F and fails
  F-atomic, and a blind put of the obligation satisfies F and fails F-collision.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 839, 840
- **Ordering note:** wave 2 of #809's split. child-1 is a prerequisite because this child's
  report fields and CLI lines sit beside child-1's in `RestoreReport` and `restore_verdict`.
  child-2 is one because this child's `Completing` arm (H-iii) decodes the record shape child-2
  defines, and both edit `multipart.rs` and `staged_protection.rs`. child-4 builds on this fence.
  Downstream, not work here: #656 should reuse this fence's batch shape for client Abort.
- **Surfaces:** data
- **Difficulty:** high
- **Do model:** opus
- **Scope:** the restore fence for `Open` sessions: one commit per session, installing the
  transition and its obligation together, holding only if the session record is still exactly
  what the pass read (the fence row's own precondition, `0016:664`) **and** the obligation key is
  absent (`require_absent`, `0016:369-373`, `:675-676`). It runs after Pass 3, before
  `emit_summary` (`restore.rs:587`); a fence fault still emits the summary (leg P3).
  `sessions_fenced` and `sessions_unsettled` go on `RestoreReport`. `needs_human()` counts the
  second, and `is_clean()` is false whenever a session was fenced, because that predicate counts
  the work the pass did (`restore.rs:189-192`). Both get audit lines.
  In `multipart.rs`, add the writer-side API the fence needs. This is the first writer of the
  session record and of a retire obligation, so update the "no writer-side constructor" notes
  (`multipart.rs:2127-2130`, `:3180-3182`) and the retire rows table (`:3141-3142`) to match what
  you add, and keep an obligation's key and payload unable to disagree.
  Update `restore_verdict` and its tests.
  One existing test changes by design: `staged_protection.rs:1742-1830` asserts the pass never
  names an undecodable session on the audit seam, which the fence makes false. Keep its
  protection assertions, and repoint its naming assertion at the unsettled line.
  One fixture changes because this child is the first to decode session values: child-1's
  `crates/custodian/tests/restore_staged_report.rs` ran in parallel with child-2, so its seeded
  sessions lack `segment_nonce`. Add the field to that file's session builder, and nothing else
  in it; if any of its legs then fails for another reason, say so in `build-notes.md` rather
  than editing around it.
  Docs: a new fence paragraph in `docs/design/architecture/06-runtime-view.md` §6.5, after `:63`
  (not §6.7: its lines are 3–6 KB each). Update the m4 blueprint's step 7
  (`m4-first-deployment-blueprint.md:581-625`), including its claim that the pass never decodes
  an `mpu:` value (`:610-612`), and `cli.rs`'s matching comment (`:1339-1345`).
  `gc.rs` only to widen the visibility of its paging helpers (`walk_staged_range` `:1714`,
  `staged_page` `:1733`) if the fence reuses them.
  Size budget: at most 9 files and under 90 KB of diff.
  / out of scope: fencing `Completing` sessions, and anything about `seg:` records (child-4;
  here a `Completing` session is only named); DST coverage (child-5; leave
  `crates/dst/tests/custodian.rs` untouched unless an existing property stops passing, and then
  say so in `build-notes.md`); any durable record that the fence ran, and any "generation"
  (#810); the retire drain (#659); Abort and Complete (#656, #658); the gateway (#508);
  `scrub.rs`, `reconstruction.rs`; any edit to 0016 or an ADR.
- **Repro instruction:** on `origin/main`, seed an `Open@3` session under `mpu:` with one
  `part:` record and run `reconcile_after_restore`. The session is still `Open@3`, no `retire:`
  key exists, and the report's `Debug` rendering has no `sessions_fenced`.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/restore_open_fence.rs`, a **NEW** file (C4-verify's
  red comes only from an added `*/tests/*.rs`, `run-verify.sh:141-144`). Keep the doubles inside
  it: any other file added under `tests/` is taken for a test target by the gate.
- **Production reach:** the pass under test is the production `reconcile_after_restore`. Every
  session is seeded by the test, because no client can create one until #508.
- **Citations expected:** Do must cite `path:line` on the target branch for every change. Peer
  callsites Do MAY open and mirror:
  * `crates/custodian/src/restore.rs:312-589` (the pass: Pass 3 at `:534-585`, `emit_summary` at
    `:587`), `:494-529` (the mark commits: attribute only after a commit lands), `:185-217`.
  * `crates/core/src/multipart.rs:2006-2039` (`SessionState`), `:2133` (`SessionRecord`),
    `:2250` (`decode_session_record`), `:1212` (`mpu_key`), `:1339` / `:1465` (`RetireMode`,
    `retire_key`), `:3129-3150` (the retire rows table), `:3296-3303` (the `all` wildcard's
    rule), `:3455` (`decode_retire_obligation`), `:4236-4250` (no part commits outside `Open`).
  * `crates/custodian/src/gc.rs:1537-1560` (the session listing walk), `:1714`, `:1733`.
  * `crates/traits/src/lib.rs:205-212` (`CommitUnknownResult`), `:1480-1489` (`CommitOutcome`).
  * Test doubles: `crates/custodian/tests/staged_protection.rs:154-400` (the store double with
    hooks, the peer for F-race's write between read and commit), `:1067` (records across pages),
    `:1440-1540` (a write landing during the restore's reads).
  * `crates/server/src/cli.rs:1256-1369`, `:1389`, tests from `:2898`.
- **Prior-art check (triage cycles):** by path (`restore.rs`, `multipart.rs`, `cli.rs`,
  `staged_protection.rs`), 2026-09-29 on `243241e`: no merged change fences sessions; no open PR,
  and no closed-unmerged PR among the last 40, touches these paths. Rejected prior art: #809
  iteration 1 (`results/issue_809/iteration-v1/`) fenced both shapes in one 179 KB patch and ran
  the fence before Pass 3 (leg P3); #664 iteration 1 dropped an already-`Aborting` session's
  findings on a second pass (leg K). Do not repeat either unchanged.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

Plan-review response (2026-09-29): all three findings revised in place. (1) Scope now requires
`require_absent` on the obligation key (`0016:369-373`, `:675-676`), and new leg F-collision seeds
a different decodable obligation under the fence's key: both records survive, the session is
named with a "key taken" cause, the pass fences the rest. (2) P3 now adds an under-replicated
chunk, whose only audit record is the summary, and requires the summary on the fence-fault path
(never reading "complete"); the CLI's `Err` path is unchanged. (3) CLI now names the first 20
through `named_records` unchanged, counts the rest, and adds a 21-session test; report and audit
trail carry every name. Also fixed while verifying: F-race's writer lands `Completing@E+1`, not
`@E` (`0016:704-708`). Following #840's placement A (the human, 2026-09-29): seeded sessions
carry `segment_nonce`, and this child adds it to child-1's session builder, since it is the
first to decode session values (budget 8 → 9 files).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Correct the audit verdict on an interrupted fence—an otherwise healthy store with its first upload still Open is certified `clean=true`; the executable counterexample contradicts the restore-summary guarantee (`crates/custodian/src/restore.rs:1315`, `reviewer-audit-probe.log:12`).; `crates/custodian/src/restore.rs:1315` (called from `:713`): when the fence fails, the summary line (the only audit record of the counts on that path) still prints `clean = report.is_clean()`, and `is_clean()` ignores `fence_finished`. **Concrete failing case, reproduced:** a store with nothing wrong in it except one `Open@3` session whose fence commit returns `CommitUnknownResult`. The pass returns `Err`, and the summary reads `clean=true needs_human=false fence_finished=false message="post-restore reconciliation INCOMPLETE — the session fence did not finish …"`. So the line certifies the run as clean while the message says it is not finished, and an `Open` session may still be live. The P3 leg hides this: `restore_open_fence.rs:815` seeds a dangling chunk, so `needs_human` is true and `clean` is false for a different reason. The leg checks `under_replicated`, `dangling` and the message (`:838`, `:844`) but never `clean`. Fix: `clean = fence_finished && report.is_clean()`, and add a P3 variant with no other finding that asserts `summary["clean"] == "false"`. (The T4 batch review found the same thing three times. This confirms it by running it.); `crates/core/src/multipart.rs:2265`: the "`None` unless `Open`" contract of the new public `SessionRecord::open_teardown` is not tested anywhere. Its only caller, `crates/custodian/src/restore.rs:800`, calls it from inside the `SessionState::Open {}` arm, so the non-`Open` branch never runs. If that guard were deleted, `open_teardown` would return a `{session, all}` teardown for a `Completing` session (the exact shape `0016:2187` limits to `Open`), and every test would still pass. Downstream callers (#656 Abort, the reaper) will rely on that guard. This is also why C5 reports 5 survivors (`multipart.rs:2265`, `:2272`, `:3622`). I applied the `delete !` and `key -> vec![]` mutants by hand, and both are killed by the custodian tests (9 and 7 failures). They survive only because the mutant run uses `wyrd-core`'s own tests, which never call the new API. Fix: a `wyrd-core` unit test that calls `open_teardown` on each state and at `u64::MAX`, and round-trips `obligation().key()` / `payload()` through `decode_retire_obligation`.; **A failed fence can emit `clean=true`.** `crates/custodian/src/restore.rs:713` now emits the summary on fence errors, but `crates/custodian/src/restore.rs:1315` derives `clean` solely from the partial report. With one `Open` session and otherwise healthy metadata, failure of the first fence commit leaves every report counter/list empty, so the audit reports `clean=true` while the session may remain publishable. The `INCOMPLETE` message does not correct that structured verdict. Require `fence_finished && report.is_clean()` for the audit field. Add an otherwise-clean fence-failure assertion: the existing fault test at `crates/custodian/tests/restore_open_fence.rs:815` seeds a dangling chunk, which independently makes `is_clean()` false and masks this bug.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_841/review-b. 4 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 44 mutants tested in 5m: 5 missed, 15 caught, 24 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_841/review-b
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
