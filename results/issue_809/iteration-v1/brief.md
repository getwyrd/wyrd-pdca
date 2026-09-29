# Brief — restore-session-fence

> Child 2 of 3 of #664's split (itself 637.4). Do reads ONLY this file. Keep the
> `- **Label:** value` lines. `path:line` citations are on `origin/main` @ `f41e9c5`
> (re-verified 2026-09-18). This bundle's base is `origin/main` **plus child-1's accepted
> patch** if child-1 ran first: child-1 edits `gc.rs` comments and one doc paragraph only, so
> re-locate by symbol. Background: the restore rows of 0016's decision-2 table
> (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:820-871`, `:823`), the failure
> table (`:874-890`, X57 at `:880`), decision 1.4 / D-B (`:717-728`), and the fence rows of the
> batch table (`:660`, `:664-665`).
>
> **Design call settled at Plan (2026-09-12, the human, option (i)):** a `Completing` session
> stores its **segment-group nonce** on its session record, beside the fence epoch
> `PublishTarget` already carries (`crates/core/src/multipart.rs:1954-1961`).
> `SessionState::Completing` carries `fenced_at_millis`, `segments_written` and
> `publish_target` today and no nonce (`multipart.rs:2008-2038`). Rejected: deriving the nonce
> from `(upload id, E)` as `0016:2333` says, because the code keeps the nonce independent of the
> upload id on purpose (`multipart.rs:3307`); and leaving the records without a deleter.

- **Slug:** restore-session-fence
- **Kind:** enhancement
- **Defect:** restore fences no session, and cannot report what it skipped. A restored image
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
- **Success criterion:** the NEW file `crates/custodian/tests/restore_session_fence.rs` passes
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
- **Falsifiability:** RED is produced in-process on the base by **assertion**: base restore
  reads no `mpu:` record and writes no fence, so E, F, G, H and K fail there; H-iii fails there
  too, because the base report has no `staged_untrusted` and reads `is_clean()` true over a held
  record. J is green-only.
  The new test may name only base-visible symbols (`wyrd_custodian::{reconcile_after_restore,
  RestoreReport}`, `wyrd_core::multipart::{mpu_key, part_key, sidx_key, retire_key, RetirePayload,
  decode_retire_obligation, decode_session_record, RetireMode, RetireToken}`,
  `wyrd_core::metadata::seg_key`, `wyrd_traits`) — no field or type this slice adds, hence the
  `Debug` assertions. A compile failure on the RED leg reports UNVERIFIABLE
  (`engine/scripts/run-verify.sh`). Record in `build-notes.md` how many tests ran red.
- **Invariant to restore:** a session a restore resurrected can no longer publish, and every
  record that session wrote has a named deleter, installed in the same commit as the fence.
  What restore skipped or could not fence, it says. Source: 0016 `:823`, D-B and decision 1.4
  (`:717-728`), X57 (`:880`), ADR-0045. SELF-TEST: fencing `Open` sessions alone leaves a
  `Completing` session's segment records with no deleter.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Repro instruction:** on the base, seed an `Open@E` session under `mpu:` with one `part:` record
  and run `reconcile_after_restore`: the session is still `Open@E`, no `retire:` key exists,
  and the report's `Debug` rendering has no `sessions_fenced`.
- **Scope:** the nonce on the `Completing` session record and its codec; the restore fence in
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
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/restore_session_fence.rs` — a **NEW** file; the
  C4-verify gate earns its red only from an added `*/tests/*.rs`. No `Cargo.toml` change.
- **Difficulty:** high
- **Conflicts with:** 808, 813, 814, 804
- **Ordering note:** wave 2 of #664's split. Needs nothing from child-1, but both edit `gc.rs`
  and the paragraph at `06-runtime-view.md:78`. child-3 builds on this child. After acceptance
  add `Conflicts with: 663, 804` (same doc). Downstream, not work here: #658 must write this
  nonce when it fences a session into `Completing`; #656 should reuse this fence batch. **Re-pointed 2026-09-19:** #663 was split at its re-plan into #813 (scrub checks committed staged fragments; reconstruction keeps their repair queued) and #814 (reconstruction rebuilds a staged chunk); the field above names both in place of `663`.
- **Surfaces:** data
- **Do model:** opus-max
- **Production reach:** the pass under test is the production `reconcile_after_restore`. Every
  session is seeded by the test, because no client can create one until #508.
- **Citations expected:** Do must cite `path:line` on the target branch for every change. Peer
  callsites Do MAY open and mirror:
  * `crates/custodian/src/restore.rs:111-215` (`MARK_BATCH`, `RestoreReport`, `pending_skipped`
    `:126`, `needs_human` `:212`), `:312` (`reconcile_after_restore`), `:438` (#803's staged
    gate), `:490`, `:511`, `:813-825` (`attribute_staged` and the `deferred: #664` marker).
  * `crates/core/src/multipart.rs:1954-1961` (`PublishTarget`), `:2008-2038` (`SessionState`),
    `:2250` (`decode_session_record`), `:3455` (`decode_retire_obligation`), `:3141` (the retire
    rows table), `:3307` (why the nonce is independent of the upload id).
  * `crates/core/src/metadata.rs:763` (`SegmentNonce`), `:798` (`SegmentGroup`), `:1258`
    (`seg_key`).
  * `crates/custodian/src/gc.rs:809` (`staged_fragments`) and the paging helpers under it.
  * `crates/server/src/cli.rs:1230-1370`, `:2905-2990`.
- **Prior-art check (triage cycles):** by path (`restore.rs`, `multipart.rs`), re-run
  2026-09-18 on `f41e9c5`: #803 (PR #807) is the only recent change to `restore.rs`; no merged
  change fences sessions; no open PR touches these paths. Rejected prior art: #637 iteration 1
  fenced `Completing` sessions without the `seg:` deleter; #664 iteration 1
  (`results/issue_664/iteration-v1/`) added the deleter but skipped an already-`Aborting`
  session on a second pass (`restore.rs:916` in that patch) — leg K. Do NOT re-attempt either
  unchanged.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Slice too big: patch 179 KB (size backstop, threshold 100 KB) and C5 mutation testing timed out at 7200s on 64 mutants with no verdict. Split at re-plan (`pdca split 809`); how to split is left to the re-plan. Carry into whichever child owns the Completing-session fence: - Size bug: `retire:bytes {session, parts}` for a sparse Completing session (~7,935+ non-adjacent parts) exceeds FDB's 100,000-byte value limit, so every restore run fails and later sessions stay unfenced. Fix options for the re-plan to pick: (a) `{session, all}` as for Open, or (b) pre-check with `flat_value_ceiling_crossed` and name the session unsettled. Add a sparse regression with a size-enforcing store double. - Run the fence after Pass 3 (DANGLING/MISPLACED) so a fence error cannot hide those verdicts (#651 class). - Test the unreadable / stray `seg:` record branch in `read_attempt` (uncovered in diff coverage). Open for the re-plan: seeded Tier-0 DST coverage for the fence (rubric requires it; the brief put `crates/dst/tests/custodian.rs` out of scope). Add it in a new DST file or record a deferral with an issue number.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Slice too big: patch 179 KB (size backstop, threshold 100 KB) and C5 mutation testing timed out at 7200s on 64 mutants with no verdict. Split at re-plan (`pdca split 809`); how to split is left to the re-plan.
  Carry into whichever child owns the Completing-session fence:
  - Size bug: `retire:bytes {session, parts}` for a sparse Completing session (~7,935+ non-adjacent parts) exceeds FDB's 100,000-byte value limit, so every restore run fails and later sessions stay unfenced. Fix options for the re-plan to pick: (a) `{session, all}` as for Open, or (b) pre-check with `flat_value_ceiling_crossed` and name the session unsettled. Add a sparse regression with a size-enforcing store double.
  - Run the fence after Pass 3 (DANGLING/MISPLACED) so a fence error cannot hide those verdicts (#651 class).
  - Test the unreadable / stray `seg:` record branch in `read_attempt` (uncovered in diff coverage).
  Open for the re-plan: seeded Tier-0 DST coverage for the fence (rubric requires it; the brief put `crates/dst/tests/custodian.rs` out of scope). Add it in a new DST file or record a deferral with an issue number.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_809/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
