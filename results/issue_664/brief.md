# Brief — issue 664 / staged-drain-and-restore-fence

> Child 4 of 4 of #637's pdca split (637.4), **re-planned 2026-09-18 after iteration 1 returned
> ITERATE_PLAN**. Do reads ONLY this file. Keep the `- **Label:** value` lines.
> `path:line` citations are on `origin/main` @ **`f41e9c5`** (re-verified 2026-09-18), **except**
> where a line is marked as iteration 1's patched file.
> *Numbering:* the tracker body says "Slice 4 of 5" — that counts the 2026-07-31 tracker slicing
> 637.1–637.5. The pdca split materialised four of them (`results/issue_637/split-lineage.json`:
> 661, 662, 663, 664); the fifth, #665 (637.5, DST race cases), stays a tracker issue and was
> not materialised (`results/issue_637/split-proposal.md:42`). Same slice, two counts.
>
> **This bundle is split and builds nothing** (`close-disposition = split`; children **#808,
> #809, #810**, `split-lineage.json`). Iteration 1 came in at **211 KB / 14 files** against a
> 100 KB backstop, and the review converged on one correctness bug in its largest part: at
> line 916 of iteration 1's *patched* `restore.rs` (`review-batch.md`, three findings;
> `iteration-v1/patch.diff`), a second fence pass skipped an already-`Aborting` session and could
> certify the generation complete over unrepaired residue. **That line is not on the base:** on
> `f41e9c5`, `restore.rs:916` is inside `emit_dangling`, and the base has no session fence and
> no generation record at all — nothing there to re-fence. The sign-off's instruction was to
> split rather than iterate-do. The children and their legs are summarised under
> `Success criterion` below. The materialised child briefs (`results/issue_80{8,9,10}/brief.md`)
> are the ones Do reads and are authoritative; `split-proposal.md` is the pre-accept draft.
>
> **Base moved since iteration 1.** #803 merged (`14646e3`, PR #807) and #661, #693, #655 with
> it, so this bundle's base is now plain `origin/main` — no wave fold. #803 built the **staged
> protection class** (`StagedSet`, `crates/custodian/src/gc.rs:672-781`, read by
> `staged_fragments` `:809`) and deliberately left this slice's two consumers alone: its commit
> message states "scrub and drain-status keep reading only committed chunk maps", and it left
> the markers `deferred: #663, #664 — … drain status and rebalance (#664), acting on staged
> bytes` at `gc.rs:669` and `crates/custodian/tests/staged_protection.rs:2160`. Those markers
> name exactly this slice's first defect.
>
> Background: the restore, rebalance and drain rows of 0016's decision-2 table
> (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:820-871`), the failure table
> `:874-890`, decision 1.4 (`:717-728`), and the fence rows of the batch table (`:664-665`).
>
> **Design call settled at Plan (2026-09-12, the human, option (i)) — carried forward:** a
> `Completing` session stores its **segment-group nonce** on its session record, alongside the
> fence epoch `PublishTarget` already carries (`crates/core/src/multipart.rs:1954-1961`). That
> lets the restore fence name the attempt's `seg:<nonce>:<E>:` records. `SessionState::Completing`
> carries `fenced_at_millis`, `segments_written` and `publish_target` today and no nonce
> (`multipart.rs:2008-2038`). Rejected: deriving the nonce from `(upload id, E)` as `0016:2333`
> says, because the code keeps the nonce independent of the upload id on purpose
> (`multipart.rs:3307`); and leaving the records without a deleter (iteration 1's outcome).

- **Slug:** staged-drain-and-restore-fence
- **Kind:** enhancement
- **Defect:** four gaps, all on the operator-facing and post-restore side.
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
- **Success criterion:** this bundle is met when all three children are accepted; it builds
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
- **Falsifiability:** RED is produced in-process on plain `origin/main` @ `f41e9c5`, no
  container, by **assertion** — the base's drain status counts committed placements only, and
  its restore reads no `mpu:` session record, writes no fence and records no generation. Each
  child's new test file may name only base-visible symbols; a compile failure on the RED leg
  reports UNVERIFIABLE (`engine/scripts/run-verify.sh`), so any counter a child adds is
  asserted through the report's `Debug` rendering rather than by field name.
- **Invariant to restore:** no answer the custodian gives about a server or a restored image
  claims more than is true. A drain is `Satisfied` only when no byte that can still become
  referenced — committed, committed-part or in-flight — lives on that server. A restored image
  is declared fenced only when every session it resurrected can no longer publish, every
  record that session wrote has a named deleter, and no later pass can withdraw that judgement
  by forgetting what an earlier one could not repair. Source: 0016 decision 2's drain,
  rebalance and restore rows (`0016:823`, `:826-827`, `:881`), D-B and decision 1.4
  (`:717-728`), X57 (`:880`); the C-1 rule that a certification over an incomplete picture is a
  defect (`docs/principles.md` §5); ADR-0045. SELF-TEST: fencing `Open` sessions alone leaves
  a `Completing` session's segment records with no deleter. Counting only `part:` in the drain
  misses the in-flight `sidx:` case.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Conflicts with:** 663, 804
- **Ordering note:** **PLAN INTAKE CAP OVERRIDDEN BY THE HUMAN, 2026-09-18.** `scripts/plan-cap
  --need 3` printed `planned 22/6 (cap) — room for 0, need 3: Plan intake closed` (PLANNED 17:
  508 625 633 663 722 738 741 742 773 774 775 776 777 778 779 800 804 · BUILT 3: 711 721 736 ·
  AWAITING_SIGNOFF 2: 682 selftest). Rule `wyrd-pdca-P1` (`docs/INTEGRATION.md` §11) closes
  Plan-side intake at room 0, including split proposals. The human overrode it in this session
  **for #664 only**, to file this split's children; the override covers these children and no
  other id. Count at override: 22 in flight, cap 6; the children take it to 25.
  **The tracker's `Depends on #662` is satisfied, not dropped.** #662 was itself split on
  2026-09-15 into #803 (the staged protection class) and #804 (mark codec + reclaim intent)
  (`results/issue_662/split-lineage.json`), and that split session re-pointed #664's edge from
  #662 to #803 (`results/issue_662/split-proposal.md:42-49`). #803 is merged (PR #807,
  `14646e3`, on `f41e9c5`). #664 needs nothing from #804: restore counts an existing `orphan:`
  key without decoding its value (`already_marked`, `restore.rs:120-123`), and no child writes or
  reads a mark shape. #804 stays in `Conflicts with` only, for the shared doc.
  **Stale fields dropped:** iteration 1 carried `Depends on: 803` and `Conflicts with: 693` —
  both are merged (#803 as PR #807 `14646e3`; #693 as PR #799 `14911ec`), so neither constrains
  this slice any more and the base is plain `origin/main`. `Conflicts with` now names **#663**
  (both slices edit `docs/design/architecture/06-runtime-view.md`, and #663 also reads the
  staged class) and **#804** (same doc; #804 declares `Conflicts with: 664` from 2026-09-15).
  #803 left one question to this slice, marked `// deferred: #664` at `restore.rs:819`: whether
  a held (untrusted) staged record should set `needs_human()`. **Decided by the human in this
  revision session (2026-09-18): no, but the report names it** — a `staged_untrusted` list on
  `RestoreReport`, counted by `is_clean()` and printed on an informational (not NEEDS-HUMAN)
  line; the chunk's fragments stay unmarked and the audit line stays (`restore.rs:1004`). This
  is #809's leg H-iii, a red leg. Why no human: once #809 fences the session its bytes are
  garbage whatever the record says, so what is left is cleanup, which is automatic work (#659),
  not a judgement. Why not silent: a damaged record points at a bug or corruption, and after
  #808 it blocks every drain in the cluster, as a malformed committed placement already does
  (`desired_state.rs:234-246`), so the operator should hear about it at restore time rather
  than when a drain stalls. (An earlier pass of this revision chose plain "no" on the
  malformed-placement precedent; the drain code's "skip + NEEDS-HUMAN" at
  `desired_state.rs:235-236` showed that precedent does not point one way, and the "#659 clears
  it" reason is not yet true — see the #659 note below.)
  Downstream notes, not work here: #658 (Complete) must write the nonce #809 adds when it
  fences a session into `Completing`; #656 (Abort) should reuse #809's fence batch rather
  than write a second one; #508 consumes #810's generation record; #659 (retire drain) says it
  reclaims owned bytes only "with a validated `StagedPlacement`" and does not say what it does
  with a cancelled upload's untrusted record — a comment asking it to clean such a record up by
  chunk id was drafted for the human on 2026-09-18; the rule behind it (keep-on-doubt protects user data, not system residue) is #811.
- **Surfaces:** data
- **Difficulty:** high
- **Do model:** opus-max
- **Scope:** carried by #808, #809 and #810 — each child's bounded scope is under `Success
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
- **Repro instruction:** on `origin/main` @ `f41e9c5`, seed an owned `sidx:` entry whose
  fragment sits on server `S`, set `desired:dserver:<S>` with `set_lifecycle`, and call
  `reconciliation_status(S)`: it answers `Satisfied`.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** none for this bundle (split). One **NEW** file per child:
  `crates/custodian/tests/staged_drain_status.rs` (#808),
  `crates/custodian/tests/restore_session_fence.rs` (#809),
  `crates/custodian/tests/restore_fence_generation.rs` (#810). The C4-verify gate earns its red
  only from an added `*/tests/*.rs` (`engine/scripts/run-verify.sh:141-144`), so no child may
  append to an existing suite. The existing
  dev-dependencies suffice; no `Cargo.toml` change.
- **Production reach:** the passes under test are the production `reconciliation_status`, the
  rebalance loop and `reconcile_after_restore`. Every session is seeded by the test, because no
  client can create one until #508. The generation record has no reader until #508's gateway
  gate. Until then the guarantee rests on the deployment ordering 0016 also allows: run the
  post-restore pass before re-enabling gateways (`0016:3017-3021`). Declared so sign-off weighs
  it rather than discovering it.
- **Citations expected:** Do must cite `path:line` on the target branch for every change. Peer
  callsites Do MAY open and mirror:
  * `crates/custodian/src/gc.rs:672-781` — `StagedSet`, its `protection` (`:690`) and `protects`
    (`:705`), and `staged_fragments` (`:809`). The class to read, not rebuild.
  * `crates/custodian/src/gc.rs:515` — `ReferenceSet::protects`, the committed twin.
  * `crates/custodian/src/desired_state.rs:181-247` — `reconciliation_status` and its
    `genuinely_holds` test (`:191`).
  * `crates/custodian/src/rebalance.rs:257` — `plan_evacuations`.
  * `crates/custodian/src/restore.rs:111-215` (`MARK_BATCH`, `RestoreReport`,
    `pending_skipped` `:126`, `needs_human` `:212`), `:312` (`reconcile_after_restore`),
    `:438` (#803's staged gate), `:490`, `:511`, `:813-825` (`attribute_staged` and the
    `deferred: #664` marker).
  * `crates/core/src/multipart.rs:1954-1961` (`PublishTarget`), `:2008-2038` (`SessionState`),
    `:2250` (`decode_session_record`), `:3455` (`decode_retire_obligation`), `:3141` (the
    retire rows table), `:3307` (why the nonce is independent of the upload id).
  * `crates/core/src/metadata.rs:763` (`SegmentNonce`), `:798` (`SegmentGroup`), `:1258`
    (`seg_key`).
  * `crates/server/src/cli.rs:1230-1370` (`restore_verdict` and the operator paragraphs) and
    `:2905-2990` (its report tests, which build `RestoreReport` literals).
- **Prior-art check (triage cycles):** by path (`crates/custodian/src/desired_state.rs`,
  `rebalance.rs`, `restore.rs`, `crates/core/src/multipart.rs`) across merged history, open and
  closed PRs, re-run 2026-09-18 on `f41e9c5`: #803 (PR #807, merged) is the only recent change
  to `restore.rs` and it explicitly excluded drain status and rebalance. No merged change
  fences sessions or adds a fence-generation record; no open PR touches these paths. Rejected
  prior art: #637 iteration 1 (`results/issue_637/iteration-v1/`) fenced `Completing` sessions
  without the `seg:` deleter, because the nonce was missing; #664 iteration 1
  (`results/issue_664/iteration-v1/`) added the deleter but let a re-fence pass skip an
  `Aborting` session with unrepaired residue.
- **Disposition hint:** likely-fix

## Iteration 1 — carry-forward (from the previous attempt)
> Every `restore.rs:916` below is a line of iteration 1's **patched** file
> (`iteration-v1/patch.diff`), not of `f41e9c5`, where that line sits in `emit_dangling`.
- Sign-off rationale: Slice is oversized (206 KB patch vs 100 KB threshold) and the advisory
  review (both rubric and adversary passes) converged on a real correctness bug:
  `crates/custodian/src/restore.rs:916` skips already-`Aborting` sessions on a second fence
  pass, so a repair-and-rerun (the documented operator remedy) can certify the restore-fence
  generation complete while a `Completing` session's orphaned `seg:` residue still has no
  deleter — a gateway trusting that marker would resume multipart verbs over an unfenced
  image. Given the size flag, treat this as a slicing problem, not an implementation bug to
  patch in place: split at re-plan (drain/rebalance staged-accounting vs. restore fencing +
  generation record vs. residue-across-generations handling) rather than iterate-do, so the
  rebuild doesn't keep producing implementation-shaped findings on an oversized diff. Carry
  into the split: residue must survive re-fencing across generations, and a
  repair-then-second-pass regression test is required before the fence-complete marker can be
  trusted.
- Failing gate: C5 surviving mutants on the bundle diff (advisory) — 74 mutants tested in 3m:
  9 missed, 25 caught, 40 unviable.
- Failing gate: T4 batched multi-pass rubric review — 3 blocking findings, all at
  `restore.rs:916`, all the same defect (see `review-batch.md`).
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md,
  check-*).
- Do NOT re-attempt the rejected approach unchanged.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

Plan-review response: all five findings revised in place (2026-09-18). (1) `restore.rs:916` is
now labelled as iteration 1's patched line, and defect 4 is restated as an absence on the base
plus a regression requirement pinned by #810 legs N/O. (2) Each child's test file, red legs and
bounded scope are now in `Success criterion`. (3) The "4 of 5" count and the #662 dependency are
reconciled in the header and `Ordering note` (#662 → #803, merged). (4) The `pending_chunks`
requirement is recorded in `Scope` as met by the base, with its regression test. (5) The
`restore.rs:819` question was decided by the human: no `needs_human()`, but the report names the
record (`staged_untrusted`); #809's leg H-iii now tests it as a red leg.
