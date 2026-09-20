# Result — issue 664 / staged-drain-and-restore-fence

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: four gaps, all on the operator-facing and post-restore side.
  1. **Drain status ignores staged bytes.** `reconciliation_status` answers `Satisfied` for a
     server holding only staged bytes, because its `genuinely_holds` test reads committed
     placements alone (`crates/custodian/src/desired_state.rs:181-196`). An operator is then
     told the server may be wiped under a live upload — the F6 trace. Its sharper form is an
     in-flight part with no `part:` record yet (`0016:827`). The class that answers this
     already exists, unread here: `StagedSet::protects` (`gc.rs:705`).
  2. **Rebalance's disjointness from the staged set is unasserted.** 0016 requires a rebalance
     pass over a draining server holding only staged fragments to plan no move and rewrite no
     `part:` record, while `reconciliation_status` answers `Pending` for that same server
     (`0016:881`, `plan_evacuations` at `crates/custodian/src/rebalance.rs:257`).
  3. **Restore fences no session, and cannot report what it skipped.** A restored image can
     resurrect an `Open` or `Completing` session whose bytes are gone, and nothing stops it
     from completing over them (D-B, `0016:717-728`, F13). A `Completing` session that had
     already written segments needs its `seg:` records retired in the **same** batch as its
     fence, or they have no deleter anywhere in the design (X57, `0016:880`). Today the session
     record cannot even name them (`multipart.rs:2008-2038`). Separately, restore now skips
     staged fragments silently through #803's gate (`restore.rs:438`); 0016 requires the report
     to say so — `staged_skipped` and `sessions_fenced` beside `pending_skipped`
     (`0016:823`; `RestoreReport`, `restore.rs:115-215`).
  4. **No durable record tells a gateway the fence has run.** 0016 requires the restore-fence
     generation to complete before any gateway serves multipart verbs on the restored image
     (`0016:723-728`, `:3017-3021`, X17b). On the base the record is simply absent:
     `reconcile_after_restore` (`restore.rs:312`) writes no fence and no generation key. The
     residue rule is a **regression requirement on the new code**, not a base defect: whatever
     writes the generation record must keep naming an already-`Aborting` session's unrepaired
     residue on every later pass, and must not read complete until it is repaired. Iteration 1
     broke exactly this (its patched `restore.rs:916`, see the header), so the requirement is
     pinned by a repair-then-second-pass test in #810 (legs N and O).
- Success criterion: this bundle is met when all three children are accepted; it builds
  nothing itself. Each child proves its part with its own NEW test file, run as
  `cargo test -p wyrd-custodian --test <file>`. Leg letters are each child's own (they are not
  one shared A–J sequence). `cargo xtask ci` green is each child's last leg, but it proves only
  the general gate — the proof is the named test file.
  * **#808 `staged_drain_status.rs`** (drain + rebalance; defects 1–2). RED on the base by
    assertion: (A) a draining server holding **only** an owned `sidx:` fragment →
    `reconciliation_status` is `Pending` (base: `Satisfied`); (B) the same with only a committed
    `part:` fragment → `Pending`; (D) a rebalance pass over a draining server holding only staged
    fragments writes no fragment and rewrites no `part:` record, **and** status is `Pending`;
    (E) an unreadable or untrusted staged record never yields `Satisfied` (the base never reads
    staged records, so it answers `Satisfied`). Guard, green on the base: (C) staged bytes on
    other servers → the drained one is still `Satisfied`.
    Scope: `desired_state.rs` reads `StagedSet`; rebalance gets a test and a comment only; no
    `restore.rs`, `multipart.rs` or `cli.rs`.
  * **#809 `restore_session_fence.rs`** (restore fence + report; defect 3). RED on the base by
    assertion, counters read through the report's `Debug` text: (E) two staged fragments →
    `staged_skipped: 2`; (F) `Open@E` → `Aborting@E+1` with its `retire:bytes` obligation in the
    same batch (a double failing that commit leaves none of the writes), `sessions_fenced: 1`;
    (G) `Completing@E` with the nonce and `seg:<nonce>:<E>:*` records → `Aborting@E+1`, one batch
    installing `retire:bytes` and `retire:records` naming exactly `seg:<nonce>:<E>`; (H) a
    no-nonce `Completing` record is left byte-identical, and a session whose `seg:` names a chunk
    no `part:` holds is still fenced — both make `needs_human()` true; (K) a second pass installs
    nothing new and **still** names the H sessions; (H-iii) an untrusted staged record is named
    by key under `staged_untrusted`, with `needs_human()` false and `is_clean()` false (decided
    below). Green-only: (J) the nonce codec round-trip. Scope: nonce + codec, both fence shapes,
    the three report fields, `restore_verdict` text; **no** generation record.
  * **#810 `restore_fence_generation.rs`** (generation record + residue; defect 4). Base is
    `origin/main` + #809. RED by assertion: (I) the record is absent before any pass, reads
    not-complete during one, complete after; a second pass advances the generation and reads
    not-complete until it finishes; (O) repair, then re-run → the missing obligation is
    installed, and only then complete (repairing one of two residue sessions is not enough).
    Paired with positive arms so they kill mutants: (M) needing a human blocks complete;
    (N) re-run with **nothing repaired** → still needs a human, still not complete — the
    iteration-1 regression; (P) the second pass shares no memory with the first; (Q) a failed
    commit mid-pass never reads complete. Scope: the record, re-evaluating fenced sessions every
    pass, docs for #508; the gateway's read of it is #508's.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: carried by #808, #809 and #810 — each child's bounded scope is under `Success
  criterion`, in full in its own brief. The whole, for the record:
  (1) drain status counting both staged classes as held, via the existing `StagedSet`;
  (2) rebalance confirmed disjoint from the staged set; (3) restore's staged accounting and its
  session fence in both shapes, with the obligations 0016's rows name in one batch each, plus
  the nonce on the `Completing` session record; (4) the durable restore-fence generation record
  and its residue handling across generations. **The tracker's "restore: its `pending_chunks`
  scan is bounded again (owned entries are disjoint)" is closed by the base, no work here:**
  owned entries live under `sidx:`, a prefix disjoint from `pending:` (`multipart.rs:1139-1142`,
  since `d986069`); `put_pending` refuses to write an owned entry under `pending:`
  (`metadata.rs:2087-2092`) and `decode_pending_entry` refuses to read one (`metadata.rs:1697`);
  so `pending_chunks`' `scan(b"pending:")` (`restore.rs:856-870`) sees only ordinary leases,
  which is the bound 0016 re-derives (`0016:823`). The regression evidence is #803's
  `gc_and_restore_never_scan_a_whole_staged_namespace` (`crates/custodian/tests/staged_protection.rs:1478`,
  leg D), which lowers the scan cap and asserts restore reads staged records only through
  per-session ranges. / out of scope: scrub and reconstruction (#663 —
  do not touch `scrub.rs` or `reconstruction.rs`); the staged class itself (#803, merged); the
  mark codec and reclaim intent (#804); the retire drain that empties the obligations (#659);
  the gateway's reading of the generation record (#508); evacuating committed segmented objects
  (#653/#722); client Abort and Complete (#656, #658); any edit to 0016 or an ADR.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — N/A — close disposition (no patch to verify)
- C3 Change: none — patch.diff
- C4 Verification (red→green): none — N/A — close disposition (no patch to verify)
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — N/A — close disposition (no patch to verify)
- T2 Shape: none — N/A — close disposition (no patch to verify)
- T3 Runtime: none — N/A — close disposition (no patch to verify)
- T4 Contribution: none — N/A — close disposition (no patch to verify)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Advisory review — SKIPPED (close disposition)

The reviewer leaf was skipped: this bundle's Plan concluded a close / no-fix disposition (split), so there is no patch to review.

- NEEDS-HUMAN — Confirm the close disposition 'split' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Confirm the close disposition 'split' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.
- [x] The claimed re-fence root cause is not grounded in the target source. `brief.md:7-11`, `brief.md:57-63`, and `brief.md:163-180` treat “`restore.rs:916` skips already-`Aborting` sessions” as a defect, but on the declared target `f41e9c5`, `crates/custodian/src/restore.rs:903-916` is `emit_dangling` and line 916 merely closes a tracing call; the target has no restore-fence implementation to re-fence. Reframe the target defect as the absent session fence/generation record, and make residue survival a regression requirement on the proposed implementation (or supply the prior-attempt evidence in `sources/`) rather than presenting an unavailable prior patch as current-source root cause.
- [x] The success criterion is not available to Do or review. `brief.md:3-4` says Do reads only this file, yet `brief.md:64-68`, `brief.md:109-126` delegate success, scope, and test identity to `split-proposal.md` and undefined “legs A–J”; `split-proposal.md` is not among the supplied inputs. `cargo xtask ci` being green proves only the general gate, not the drain, fence, durability, or second-pass behaviors. Put each child’s explicit red→green command/assertions and bounded scope in this brief (or provide the referenced split artifact).
- [x] The tracker identity and prerequisite were silently changed. The sole thread body says, “Slice 4 of 5 of #637. Depends on #662,” while `brief.md:3` calls this child 4 of 4 and the brief declares no `Depends on: 662`; `brief.md:95-97` explains dropping #803, not #662. No `dependency-state.json` was supplied, so #662’s existence/state cannot be resolved from the permitted record. Reconcile the 4-of-5/4-of-4 mismatch and restore #662 as a declared dependency or record why it is satisfied/superseded.
- [x] One tracker requirement disappears without disposition: `notes.json:1` requires restore’s “`pending_chunks` scan [to be] bounded again (owned entries are disjoint).” The target now makes `sidx:` disjoint from `pending:` (`crates/core/src/multipart.rs:1139-1142`) while `pending_chunks` scans only `pending:` (`crates/custodian/src/restore.rs:856-870`), so this appears already satisfied by the moved base, but `brief.md:37-63` and `brief.md:109-118` neither retain it nor explicitly close it as base-provided. State that resolution and its regression evidence, or include the remaining work.
- [x] Child 2 contains an unstated design decision: `brief.md:100-102` assigns it the question whether an untrusted staged record changes `RestoreReport::needs_human`, while the target explicitly leaves “whether” unresolved (`crates/custodian/src/restore.rs:819-821`). None of the four defects or the delegated criterion says which verdict is required or how to test it. Decide and expose that behavior as a criterion, or keep it out of this split; otherwise this is hidden scope inside “restore report work.”

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-09-15

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 5 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
